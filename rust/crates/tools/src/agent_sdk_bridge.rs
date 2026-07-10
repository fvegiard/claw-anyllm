//! Bridge claw's `Agent` tool to `@anthropic-ai/claude-agent-sdk` TypeScript orchestrator.
//!
//! When `CLAW_AGENT_SDK=1` or `subagent_type` starts with `sdk:`, delegation uses the
//! full SDK (nested subagents, Workflow, hooks, MCP) instead of the in-process Rust thread.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Input mirrored from the `Agent` tool.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct AgentSdkBridgeInput {
    pub description: String,
    pub prompt: String,
    #[serde(default)]
    pub subagent_type: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
}

/// Whether SDK delegation is explicitly disabled.
#[must_use]
pub fn sdk_explicitly_disabled() -> bool {
    std::env::var("CLAW_AGENT_SDK")
        .ok()
        .is_some_and(|value| matches!(value.to_ascii_lowercase().as_str(), "0" | "false" | "no"))
}

/// Whether the orchestrator directory and Node toolchain are available.
#[must_use]
pub fn agent_sdk_available() -> bool {
    resolve_orchestrator_root().is_ok()
        && std::process::Command::new("npx")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
}

/// Whether SDK is the default Agent path (opt-out via `CLAW_AGENT_SDK=0`).
#[must_use]
pub fn sdk_default_enabled() -> bool {
    !sdk_explicitly_disabled() && agent_sdk_available()
}

/// Whether this Agent invocation should use the TypeScript SDK orchestrator.
#[must_use]
pub fn should_delegate_to_agent_sdk(subagent_type: Option<&str>) -> bool {
    if sdk_explicitly_disabled() {
        return subagent_type.is_some_and(|subagent| {
            let lower = subagent.to_ascii_lowercase();
            lower.starts_with("sdk:")
                || lower == "agent-sdk"
                || lower == "vibe-orchestrator"
                || lower == "vision-looker"
                || lower == "github-researcher"
        });
    }

    if sdk_default_enabled() {
        return true;
    }

    if std::env::var("CLAW_AGENT_SDK")
        .ok()
        .is_some_and(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
    {
        return true;
    }

    subagent_type.is_some_and(|subagent| {
        let lower = subagent.to_ascii_lowercase();
        lower.starts_with("sdk:")
            || lower == "agent-sdk"
            || lower == "vibe-orchestrator"
            || lower == "vision-looker"
            || lower == "github-researcher"
    })
}

fn resolve_orchestrator_root() -> Result<PathBuf, String> {
    if let Ok(path) = std::env::var("CLAW_AGENT_SDK_ORCHESTRATOR") {
        let root = PathBuf::from(path);
        if root.join("package.json").is_file() {
            return Ok(root);
        }
        return Err(format!(
            "CLAW_AGENT_SDK_ORCHESTRATOR does not contain package.json: {}",
            root.display()
        ));
    }

    let cwd = std::env::current_dir().map_err(|error| error.to_string())?;
    for relative in [
        "examples/agent-sdk-orchestrator",
        "../examples/agent-sdk-orchestrator",
    ] {
        let candidate = cwd.join(relative);
        if candidate.join("package.json").is_file() {
            return Ok(candidate);
        }
    }

    Err(String::from(
        "Agent SDK orchestrator not found. Set CLAW_AGENT_SDK_ORCHESTRATOR to examples/agent-sdk-orchestrator",
    ))
}

fn normalize_sdk_agent(subagent_type: Option<&str>, name: Option<&str>) -> Option<String> {
    if let Some(raw) = subagent_type {
        let trimmed = raw.trim();
        if let Some(rest) = trimmed.strip_prefix("sdk:") {
            return Some(rest.to_string());
        }
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    name.map(str::to_string)
}

/// Run the TypeScript orchestrator and return its JSON stdout.
pub fn run_agent_sdk_bridge(input: &AgentSdkBridgeInput) -> Result<String, String> {
    let root = resolve_orchestrator_root()?;
    let agent = normalize_sdk_agent(input.subagent_type.as_deref(), input.name.as_deref());
    let cwd = input.cwd.clone().or_else(|| {
        std::env::current_dir()
            .ok()
            .map(|path| path.display().to_string())
    });

    let payload = serde_json::json!({
        "prompt": input.prompt,
        "description": input.description,
        "agent": agent,
        "cwd": cwd,
        "include_mcp": true,
        "model_hint": input.model,
    });

    let mut child = Command::new("npx")
        .args(["tsx", "src/orchestrator.ts"])
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to spawn agent sdk orchestrator via npx tsx: {error}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        let body = serde_json::to_string(&payload).map_err(|error| error.to_string())?;
        stdin
            .write_all(body.as_bytes())
            .map_err(|error| error.to_string())?;
    }

    let output = child
        .wait_with_output()
        .map_err(|error| error.to_string())?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if stdout.is_empty() {
        return Err(if stderr.is_empty() {
            String::from("agent sdk orchestrator returned empty stdout")
        } else {
            format!("agent sdk orchestrator failed: {stderr}")
        });
    }

    Ok(stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delegates_when_env_enabled() {
        assert!(should_delegate_to_agent_sdk(Some("sdk:vibe-orchestrator")));
        assert!(should_delegate_to_agent_sdk(Some("agent-sdk")));
        if sdk_default_enabled() {
            assert!(should_delegate_to_agent_sdk(Some("Explore")));
        } else {
            assert!(!should_delegate_to_agent_sdk(Some("Explore")));
        }
    }

    #[test]
    fn sdk_default_enabled_when_orchestrator_present() {
        if agent_sdk_available() {
            assert!(sdk_default_enabled() || sdk_explicitly_disabled());
        }
    }
}
