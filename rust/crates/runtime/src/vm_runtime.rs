//! Linux coding VM detection and docker-compose helpers.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::sandbox::{detect_container_environment_from, SandboxDetectionInputs};

pub const CLAW_VM_MARKER: &str = "CLAW_VM";
pub const CLAW_VM_COMPOSE_SERVICE: &str = "claw-vm";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct VmStatus {
    pub running: bool,
    pub in_vm: bool,
    pub workspace_root: String,
    pub active_project: Option<String>,
    pub fleet_count: usize,
    pub compose_project: Option<String>,
}

/// Whether the current process is inside the claw Linux coding VM.
#[must_use]
pub fn in_claw_vm() -> bool {
    if std::env::var(CLAW_VM_MARKER)
        .ok()
        .is_some_and(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
    {
        return true;
    }
    let env_pairs: Vec<(String, String)> = std::env::vars().collect();
    let dockerenv = Path::new("/.dockerenv").exists();
    let containerenv = Path::new("/run/.containerenv").exists();
    let cgroup = std::fs::read_to_string("/proc/1/cgroup").ok();
    let container = detect_container_environment_from(SandboxDetectionInputs {
        env_pairs,
        dockerenv_exists: dockerenv,
        containerenv_exists: containerenv,
        proc_1_cgroup: cgroup.as_deref(),
    });
    container.in_container
}

pub fn default_workspace_root() -> PathBuf {
    if in_claw_vm() {
        PathBuf::from(crate::project_router::VM_PROJECTS_ROOT)
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(".claw")
            .join("projects")
    }
}

fn resolve_compose_file() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    [
        cwd.join("docker-compose.yml"),
        cwd.join("compose.yml"),
        cwd.join("../docker-compose.yml"),
    ]
    .into_iter()
    .find(|candidate| candidate.is_file())
}

fn compose_base_args(compose_file: &Path) -> Vec<String> {
    vec![
        String::from("compose"),
        String::from("-f"),
        compose_file.display().to_string(),
    ]
}

/// Start the claw-vm service via docker compose.
pub fn vm_up() -> Result<String, String> {
    if in_claw_vm() {
        return Ok(String::from("already inside claw VM"));
    }
    let compose_file = resolve_compose_file().ok_or_else(|| {
        String::from("docker-compose.yml not found; run from repo root or set CLAW_VM_COMPOSE")
    })?;
    let mut args = compose_base_args(&compose_file);
    args.push(String::from("up"));
    args.push(String::from("-d"));
    args.push(CLAW_VM_COMPOSE_SERVICE.to_string());
    let output = Command::new("docker")
        .args(&args)
        .output()
        .map_err(|e| format!("docker compose up failed: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(format!(
            "docker compose up failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

/// Execute a command inside the claw-vm container.
pub fn vm_exec(command: &str) -> Result<String, String> {
    if in_claw_vm() {
        let output = Command::new("sh")
            .arg("-lc")
            .arg(command)
            .output()
            .map_err(|e| format!("local exec failed: {e}"))?;
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).to_string());
        }
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    let compose_file =
        resolve_compose_file().ok_or_else(|| String::from("docker-compose.yml not found"))?;
    let mut args = compose_base_args(&compose_file);
    args.push(String::from("exec"));
    args.push(String::from("-T"));
    args.push(CLAW_VM_COMPOSE_SERVICE.to_string());
    args.push(String::from("sh"));
    args.push(String::from("-lc"));
    args.push(command.to_string());
    let output = Command::new("docker")
        .args(&args)
        .output()
        .map_err(|e| format!("docker compose exec failed: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

pub fn vm_status(claw_home: &Path) -> VmStatus {
    let workspace_root = default_workspace_root();
    let registry = crate::project_router::load_registry(claw_home);
    let active_project = registry
        .projects
        .iter()
        .max_by_key(|p| p.last_used_at.as_deref().unwrap_or("").to_string())
        .map(|p| p.path.clone());
    let fleet_count = crate::fleet_spawn::list_fleet_workers(claw_home).len();
    let running = in_claw_vm() || vm_container_running().unwrap_or(false);
    VmStatus {
        running,
        in_vm: in_claw_vm(),
        workspace_root: workspace_root.display().to_string(),
        active_project,
        fleet_count,
        compose_project: resolve_compose_file().map(|p| p.display().to_string()),
    }
}

fn vm_container_running() -> Option<bool> {
    let compose_file = resolve_compose_file()?;
    let mut args = compose_base_args(&compose_file);
    args.push(String::from("ps"));
    args.push(String::from("--status"));
    args.push(String::from("running"));
    args.push(CLAW_VM_COMPOSE_SERVICE.to_string());
    let output = Command::new("docker").args(&args).output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    Some(stdout.contains(CLAW_VM_COMPOSE_SERVICE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_workspace_root_is_path() {
        let root = default_workspace_root();
        assert!(!root.as_os_str().is_empty());
    }
}
