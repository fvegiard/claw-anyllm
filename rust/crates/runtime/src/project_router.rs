//! Auto route or create projects — user never runs `git init` manually.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

/// Default workspace root inside the Linux coding VM.
pub const VM_PROJECTS_ROOT: &str = "/workspace/projects";

/// Registry file tracking routed/created projects.
pub const PROJECTS_REGISTRY: &str = ".claw/projects.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectRouteAction {
    Route,
    Clone,
    Scaffold,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectEntry {
    pub id: String,
    pub name: String,
    pub path: String,
    pub origin: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub created_at: String,
    #[serde(default)]
    pub last_used_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ProjectsRegistry {
    pub projects: Vec<ProjectEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectRouteRequest {
    pub intent: String,
    #[serde(default)]
    pub repo_url: Option<String>,
    #[serde(default)]
    pub project_name: Option<String>,
    #[serde(default)]
    pub stack: Option<String>,
    #[serde(default)]
    pub workspace_root: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectRouteResult {
    pub action: ProjectRouteAction,
    pub project_id: String,
    pub project_name: String,
    pub worktree: String,
    pub repo: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRouterError {
    message: String,
}

impl ProjectRouterError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ProjectRouterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ProjectRouterError {}

pub fn resolve_projects_root(request: &ProjectRouteRequest) -> PathBuf {
    request
        .workspace_root
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            if crate::vm_runtime::in_claw_vm() {
                PathBuf::from(VM_PROJECTS_ROOT)
            } else {
                std::env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .join(".claw")
                    .join("projects")
            }
        })
}

pub fn registry_path(claw_home: &Path) -> PathBuf {
    claw_home.join(
        PROJECTS_REGISTRY
            .strip_prefix(".claw/")
            .unwrap_or(PROJECTS_REGISTRY),
    )
}

pub fn load_registry(claw_home: &Path) -> ProjectsRegistry {
    let path = registry_path(claw_home);
    if !path.is_file() {
        return ProjectsRegistry::default();
    }
    let raw = fs::read_to_string(&path).unwrap_or_default();
    serde_json::from_str(&raw).unwrap_or_default()
}

pub fn save_registry(
    claw_home: &Path,
    registry: &ProjectsRegistry,
) -> Result<(), ProjectRouterError> {
    let path = registry_path(claw_home);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| ProjectRouterError::new(e.to_string()))?;
    }
    let body = serde_json::to_string_pretty(registry)
        .map_err(|e| ProjectRouterError::new(e.to_string()))?;
    fs::write(&path, body).map_err(|e| ProjectRouterError::new(e.to_string()))
}

fn slugify(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    let mut out = String::new();
    let mut last_dash = false;
    for ch in lower.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn infer_project_name(intent: &str, hint: Option<&str>) -> String {
    if let Some(name) = hint.filter(|n| !n.trim().is_empty()) {
        return slugify(name);
    }
    let words: Vec<&str> = intent
        .split_whitespace()
        .filter(|w| w.len() > 3)
        .take(3)
        .collect();
    if words.is_empty() {
        return String::from("claw-project");
    }
    slugify(&words.join("-"))
}

fn infer_stack(intent: &str, hint: Option<&str>) -> String {
    if let Some(stack) = hint.filter(|s| !s.trim().is_empty()) {
        return stack.to_string();
    }
    let lower = intent.to_ascii_lowercase();
    if lower.contains("rust") {
        return String::from("rust");
    }
    if lower.contains("react") || lower.contains("next") || lower.contains("node") {
        return String::from("node");
    }
    String::from("static-site")
}

fn find_existing_match(intent: &str, registry: &ProjectsRegistry) -> Option<ProjectEntry> {
    let lower = intent.to_ascii_lowercase();
    registry
        .projects
        .iter()
        .filter(|p| Path::new(&p.path).exists())
        .max_by_key(|p| {
            let name_score = if lower.contains(&p.name.to_ascii_lowercase()) {
                10
            } else {
                0
            };
            let tag_score = p
                .tags
                .iter()
                .filter(|t| lower.contains(&t.to_ascii_lowercase()))
                .count();
            name_score + tag_score
        })
        .filter(|p| {
            lower.contains(&p.name.to_ascii_lowercase())
                || p.tags
                    .iter()
                    .any(|t| lower.contains(&t.to_ascii_lowercase()))
                || lower.contains("continue")
                || lower.contains("yesterday")
                || lower.contains("last project")
        })
        .cloned()
        .or_else(|| {
            registry
                .projects
                .iter()
                .filter(|p| Path::new(&p.path).exists())
                .max_by_key(|p| p.last_used_at.as_deref().unwrap_or("").to_string())
                .filter(|_| lower.contains("continue") || lower.contains("yesterday"))
                .cloned()
        })
}

fn run_git(args: &[&str], cwd: &Path) -> Result<(), ProjectRouterError> {
    let status = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .status()
        .map_err(|e| ProjectRouterError::new(format!("git spawn failed: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(ProjectRouterError::new(format!(
            "git {} failed with {}",
            args.join(" "),
            status
        )))
    }
}

fn clone_repo(url: &str, dest: &Path) -> Result<(), ProjectRouterError> {
    if dest.exists() {
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| ProjectRouterError::new(e.to_string()))?;
    }
    let status = Command::new("git")
        .args(["clone", url, &dest.display().to_string()])
        .status()
        .map_err(|e| ProjectRouterError::new(format!("git clone spawn failed: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(ProjectRouterError::new(format!("git clone failed: {url}")))
    }
}

fn scaffold_project(dest: &Path, stack: &str) -> Result<(), ProjectRouterError> {
    if dest.exists() {
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| ProjectRouterError::new(e.to_string()))?;
    }
    fs::create_dir_all(dest).map_err(|e| ProjectRouterError::new(e.to_string()))?;
    run_git(&["init"], dest)?;
    match stack {
        "rust" => {
            fs::write(
                dest.join("Cargo.toml"),
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n",
            )
            .map_err(|e| ProjectRouterError::new(e.to_string()))?;
            fs::create_dir_all(dest.join("src"))
                .map_err(|e| ProjectRouterError::new(e.to_string()))?;
            fs::write(
                dest.join("src").join("main.rs"),
                "fn main() {\n    println!(\"Hello from claw\");\n}\n",
            )
            .map_err(|e| ProjectRouterError::new(e.to_string()))?;
        }
        "node" => {
            fs::write(
                dest.join("package.json"),
                r#"{"name":"app","version":"0.1.0","private":true,"scripts":{"dev":"node index.js"}}"#,
            )
            .map_err(|e| ProjectRouterError::new(e.to_string()))?;
            fs::write(dest.join("index.js"), "console.log('Hello from claw');\n")
                .map_err(|e| ProjectRouterError::new(e.to_string()))?;
        }
        _ => {
            fs::write(
                dest.join("index.html"),
                "<!DOCTYPE html><html><body><h1>Claw project</h1></body></html>",
            )
            .map_err(|e| ProjectRouterError::new(e.to_string()))?;
        }
    }
    Ok(())
}

fn touch_last_used(entry: &mut ProjectEntry) {
    entry.last_used_at = Some(chrono_like_now());
}

fn chrono_like_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    secs.to_string()
}

fn register_project(claw_home: &Path, entry: ProjectEntry) -> Result<(), ProjectRouterError> {
    let mut registry = load_registry(claw_home);
    if let Some(existing) = registry.projects.iter_mut().find(|p| p.id == entry.id) {
        *existing = entry;
    } else {
        registry.projects.push(entry);
    }
    save_registry(claw_home, &registry)
}

/// Route or create a project from user intent.
pub fn route_project(
    claw_home: &Path,
    request: &ProjectRouteRequest,
) -> Result<ProjectRouteResult, ProjectRouterError> {
    let projects_root = resolve_projects_root(request);
    fs::create_dir_all(&projects_root).map_err(|e| ProjectRouterError::new(e.to_string()))?;

    let registry = load_registry(claw_home);
    if let Some(mut existing) = find_existing_match(&request.intent, &registry) {
        touch_last_used(&mut existing);
        register_project(claw_home, existing.clone())?;
        return Ok(ProjectRouteResult {
            action: ProjectRouteAction::Route,
            project_id: existing.id.clone(),
            project_name: existing.name.clone(),
            worktree: existing.path.clone(),
            repo: existing.origin.clone(),
            message: format!("Routed to existing project {}", existing.name),
        });
    }

    if let Some(url) = request.repo_url.as_deref().filter(|u| !u.is_empty()) {
        let name = infer_project_name(&request.intent, request.project_name.as_deref());
        let dest = projects_root.join(&name);
        clone_repo(url, &dest)?;
        let entry = ProjectEntry {
            id: name.clone(),
            name: name.clone(),
            path: dest.display().to_string(),
            origin: url.to_string(),
            tags: vec![String::from("cloned")],
            created_at: chrono_like_now(),
            last_used_at: Some(chrono_like_now()),
        };
        register_project(claw_home, entry.clone())?;
        return Ok(ProjectRouteResult {
            action: ProjectRouteAction::Clone,
            project_id: entry.id,
            project_name: entry.name,
            worktree: entry.path,
            repo: entry.origin,
            message: format!("Cloned {url}"),
        });
    }

    let stack = infer_stack(&request.intent, request.stack.as_deref());
    let name = infer_project_name(&request.intent, request.project_name.as_deref());
    let dest = projects_root.join(&name);
    scaffold_project(&dest, &stack)?;
    let entry = ProjectEntry {
        id: name.clone(),
        name: name.clone(),
        path: dest.display().to_string(),
        origin: format!("scaffold:{stack}"),
        tags: vec![stack.clone()],
        created_at: chrono_like_now(),
        last_used_at: Some(chrono_like_now()),
    };
    register_project(claw_home, entry.clone())?;
    Ok(ProjectRouteResult {
        action: ProjectRouteAction::Scaffold,
        project_id: entry.id,
        project_name: entry.name,
        worktree: entry.path,
        repo: entry.origin,
        message: format!("Scaffolded {stack} project at {}", dest.display()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_claw_home() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("claw-router-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp claw home");
        dir
    }

    #[test]
    fn scaffolds_when_no_match() {
        let home = temp_claw_home();
        let root = home.join("projects");
        let result = route_project(
            &home,
            &ProjectRouteRequest {
                intent: String::from("build me a todo app"),
                repo_url: None,
                project_name: Some(String::from("todo-app")),
                stack: Some(String::from("node")),
                workspace_root: Some(root.display().to_string()),
            },
        )
        .expect("scaffold");
        assert_eq!(result.action, ProjectRouteAction::Scaffold);
        assert!(Path::new(&result.worktree).join("package.json").is_file());
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn routes_existing_project() {
        let home = temp_claw_home();
        let root = home.join("projects");
        let project_path = root.join("my-app");
        fs::create_dir_all(&project_path).expect("project dir");
        let registry = ProjectsRegistry {
            projects: vec![ProjectEntry {
                id: String::from("my-app"),
                name: String::from("my-app"),
                path: project_path.display().to_string(),
                origin: String::from("scaffold:node"),
                tags: vec![String::from("todo")],
                created_at: String::from("1"),
                last_used_at: None,
            }],
        };
        save_registry(&home, &registry).expect("save");
        let result = route_project(
            &home,
            &ProjectRouteRequest {
                intent: String::from("continue my-app todo list"),
                repo_url: None,
                project_name: None,
                stack: None,
                workspace_root: Some(root.display().to_string()),
            },
        )
        .expect("route");
        assert_eq!(result.action, ProjectRouteAction::Route);
        let _ = fs::remove_dir_all(&home);
    }
}
