//! Doctor health probes for orchestrator stack.

use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthProbe {
    pub ok: bool,
    pub message: String,
    pub details: Vec<String>,
}

fn command_ok(cmd: &str, args: &[&str]) -> bool {
    Command::new(cmd)
        .args(args)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn resolve_orchestrator_root() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("CLAW_AGENT_SDK_ORCHESTRATOR") {
        let root = PathBuf::from(path);
        if root.join("package.json").is_file() {
            return Some(root);
        }
    }
    let cwd = std::env::current_dir().ok()?;
    for relative in [
        "examples/agent-sdk-orchestrator",
        "../examples/agent-sdk-orchestrator",
    ] {
        let candidate = cwd.join(relative);
        if candidate.join("package.json").is_file() {
            return Some(candidate);
        }
    }
    None
}

#[must_use]
pub fn probe_agent_sdk() -> HealthProbe {
    let mut details = Vec::new();
    let node_ok = command_ok("node", &["--version"]);
    details.push(format!(
        "node           {}",
        if node_ok { "ok" } else { "missing" }
    ));
    let npx_ok = command_ok("npx", &["--version"]);
    details.push(format!(
        "npx            {}",
        if npx_ok { "ok" } else { "missing" }
    ));
    let orchestrator = resolve_orchestrator_root();
    let orch_ok = orchestrator.is_some();
    if let Some(root) = &orchestrator {
        details.push(format!("orchestrator    {}", root.display()));
        let node_modules = root.join("node_modules");
        details.push(format!(
            "npm install    {}",
            if node_modules.is_dir() {
                "ok"
            } else {
                "run npm install in orchestrator dir"
            }
        ));
    } else {
        details.push(String::from("orchestrator    not found"));
    }
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .is_some();
    details.push(format!(
        "ANTHROPIC_API_KEY {}",
        if api_key { "set" } else { "missing" }
    ));
    let ok = node_ok && npx_ok && orch_ok;
    HealthProbe {
        ok,
        message: if ok {
            String::from("Agent SDK orchestrator prerequisites available")
        } else {
            String::from("Agent SDK orchestrator missing dependencies")
        },
        details,
    }
}

#[must_use]
pub fn probe_python_eval() -> HealthProbe {
    let mut details = Vec::new();
    let python_ok = command_ok("python3", &["--version"]);
    details.push(format!(
        "python3        {}",
        if python_ok { "ok" } else { "missing" }
    ));
    let root = resolve_orchestrator_root()
        .map(|p| p.join("python"))
        .unwrap_or_else(|| PathBuf::from("examples/agent-sdk-orchestrator/python"));
    let module_ok = root.join("claw_eval").join("decision_engine.py").is_file();
    details.push(format!(
        "claw_eval      {}",
        if module_ok { "ok" } else { "missing" }
    ));
    let numpy_ok = Command::new("python3")
        .args(["-c", "import numpy"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    details.push(format!(
        "numpy          {}",
        if numpy_ok {
            "ok"
        } else {
            "optional (pip install numpy)"
        }
    ));
    let ok = python_ok && module_ok;
    HealthProbe {
        ok,
        message: if ok {
            String::from("Python IF/vision evaluator available")
        } else {
            String::from("Python evaluator not ready")
        },
        details,
    }
}

#[must_use]
pub fn probe_fleet_deps() -> HealthProbe {
    let git_ok = command_ok("git", &["--version"]);
    let tmux_ok = command_ok("tmux", &["-V"]);
    let zsh_ok = command_ok("zsh", &["--version"]);
    let details = vec![
        format!("git            {}", if git_ok { "ok" } else { "missing" }),
        format!("tmux           {}", if tmux_ok { "ok" } else { "missing" }),
        format!("zsh            {}", if zsh_ok { "ok" } else { "missing" }),
    ];
    let ok = git_ok && tmux_ok;
    HealthProbe {
        ok,
        message: if ok {
            String::from("Fleet spawn dependencies available")
        } else {
            String::from("Fleet spawn missing git or tmux")
        },
        details,
    }
}

#[must_use]
pub fn probe_linux_vm(claw_home: &Path) -> HealthProbe {
    let status = crate::vm_runtime::vm_status(claw_home);
    let mut details = vec![
        format!("in_vm          {}", status.in_vm),
        format!("running        {}", status.running),
        format!("workspace      {}", status.workspace_root),
    ];
    if let Some(project) = &status.active_project {
        details.push(format!("active_project {project}"));
    }
    let writable = Path::new(&status.workspace_root)
        .parent()
        .map(|p| p.exists())
        .unwrap_or(true);
    let ok = status.in_vm || status.running || writable;
    HealthProbe {
        ok,
        message: if status.in_vm {
            String::from("Running inside claw Linux coding VM")
        } else if status.running {
            String::from("claw-vm container is running")
        } else {
            String::from("Run `claw vm up` to start Linux coding VM")
        },
        details,
    }
}

#[must_use]
pub fn probe_project_router(claw_home: &Path) -> HealthProbe {
    let root = crate::vm_runtime::default_workspace_root();
    let registry = crate::project_router::load_registry(claw_home);
    let writable = std::fs::create_dir_all(&root).is_ok();
    let details = vec![
        format!("workspace      {}", root.display()),
        format!("registry_count {}", registry.projects.len()),
        format!("writable       {writable}"),
    ];
    HealthProbe {
        ok: writable,
        message: if writable {
            String::from("Project router workspace ready")
        } else {
            String::from("Project router workspace not writable")
        },
        details,
    }
}

#[must_use]
pub fn probe_todo_discipline(claw_home: &Path) -> HealthProbe {
    let todos_path = claw_home.join("todos.json");
    let writable = std::fs::create_dir_all(claw_home).is_ok();
    let exists = todos_path.is_file();
    let details = vec![
        format!(
            "todos.json     {}",
            if exists {
                "present"
            } else {
                "will create on first use"
            }
        ),
        format!("writable       {writable}"),
    ];
    HealthProbe {
        ok: writable,
        message: String::from("TodoWrite persistence path ready"),
        details,
    }
}
