//! Fleet spawner — up to 20 isolated workers via git worktree + tmux.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

pub const MAX_FLEET_WORKERS: usize = 20;
const FLEET_STATE_FILE: &str = "fleet-state.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FleetWorker {
    pub id: String,
    pub worktree: String,
    pub branch: String,
    pub tmux_session: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct FleetState {
    pub workers: Vec<FleetWorker>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FleetSpawnError {
    message: String,
}

impl FleetSpawnError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for FleetSpawnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for FleetSpawnError {}

fn fleet_state_path(claw_home: &Path) -> PathBuf {
    claw_home.join(FLEET_STATE_FILE)
}

pub fn load_fleet_state(claw_home: &Path) -> FleetState {
    let path = fleet_state_path(claw_home);
    if !path.is_file() {
        return FleetState::default();
    }
    let raw = fs::read_to_string(&path).unwrap_or_default();
    serde_json::from_str(&raw).unwrap_or_default()
}

pub fn save_fleet_state(claw_home: &Path, state: &FleetState) -> Result<(), FleetSpawnError> {
    fs::create_dir_all(claw_home).map_err(|e| FleetSpawnError::new(e.to_string()))?;
    let body =
        serde_json::to_string_pretty(state).map_err(|e| FleetSpawnError::new(e.to_string()))?;
    fs::write(fleet_state_path(claw_home), body).map_err(|e| FleetSpawnError::new(e.to_string()))
}

pub fn list_fleet_workers(claw_home: &Path) -> Vec<FleetWorker> {
    load_fleet_state(claw_home).workers
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn now_secs() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| String::from("0"))
}

fn tmux_available() -> bool {
    Command::new("tmux")
        .arg("-V")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FleetSpawnRequest {
    pub prompt: String,
    #[serde(default)]
    pub worker_id: Option<String>,
    #[serde(default)]
    pub project_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FleetSpawnResult {
    pub worker: FleetWorker,
    pub message: String,
}

/// Spawn a fleet worker in an isolated worktree + tmux session.
pub fn spawn_fleet_worker(
    claw_home: &Path,
    project_root: &Path,
    request: &FleetSpawnRequest,
) -> Result<FleetSpawnResult, FleetSpawnError> {
    if !git_available() {
        return Err(FleetSpawnError::new("git not available on PATH"));
    }
    if !tmux_available() {
        return Err(FleetSpawnError::new("tmux not available on PATH"));
    }

    let mut state = load_fleet_state(claw_home);
    if state.workers.len() >= MAX_FLEET_WORKERS {
        return Err(FleetSpawnError::new(format!(
            "fleet cap reached ({MAX_FLEET_WORKERS} workers)"
        )));
    }

    let id = request
        .worker_id
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("fleet-{}", state.workers.len() + 1));

    let worktree_dir = project_root.join(".claw").join("fleet").join(&id);
    let branch = format!("claw/fleet-{id}");
    let tmux_session = format!("claw-fleet-{id}");

    if let Some(parent) = worktree_dir.parent() {
        fs::create_dir_all(parent).map_err(|e| FleetSpawnError::new(e.to_string()))?;
    }

    if !worktree_dir.exists() {
        let status = Command::new("git")
            .args([
                "worktree",
                "add",
                &worktree_dir.display().to_string(),
                "-b",
                &branch,
            ])
            .current_dir(project_root)
            .status()
            .map_err(|e| FleetSpawnError::new(format!("git worktree failed: {e}")))?;
        if !status.success() {
            return Err(FleetSpawnError::new("git worktree add failed"));
        }
    }

    let claw_inner = format!(
        "claw --output-format json prompt {}",
        shell_single_quote(&request.prompt)
    );
    let claw_cmd = format!("zsh -l -c {}", shell_single_quote(&claw_inner));
    let tmux_cmd = format!(
        "tmux new-session -d -s {tmux_session} -c {} {claw_cmd}",
        worktree_dir.display()
    );
    let status = Command::new("sh")
        .arg("-lc")
        .arg(&tmux_cmd)
        .status()
        .map_err(|e| FleetSpawnError::new(format!("tmux spawn failed: {e}")))?;
    if !status.success() {
        return Err(FleetSpawnError::new("tmux new-session failed"));
    }

    let worker = FleetWorker {
        id: id.clone(),
        worktree: worktree_dir.display().to_string(),
        branch,
        tmux_session,
        created_at: now_secs(),
    };
    if let Some(existing) = state.workers.iter_mut().find(|w| w.id == id) {
        *existing = worker.clone();
    } else {
        state.workers.push(worker.clone());
    }
    save_fleet_state(claw_home, &state)?;

    Ok(FleetSpawnResult {
        worker,
        message: format!("Spawned fleet worker {id}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fleet_state_roundtrip() {
        let dir = std::env::temp_dir().join(format!("claw-fleet-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("dir");
        let state = FleetState {
            workers: vec![FleetWorker {
                id: String::from("w1"),
                worktree: String::from("/tmp/w1"),
                branch: String::from("claw/fleet-w1"),
                tmux_session: String::from("claw-fleet-w1"),
                created_at: String::from("1"),
            }],
        };
        save_fleet_state(&dir, &state).expect("save");
        assert_eq!(load_fleet_state(&dir).workers.len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }
}
