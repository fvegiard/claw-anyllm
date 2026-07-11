//! Completion gate — block "finished" until user-ask matrix passes.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GateStatus {
    Pass,
    Fail,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GateMatrixRow {
    pub requirement: String,
    pub phase: String,
    pub status: GateStatus,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompletionGateReport {
    pub ready: bool,
    pub matrix: Vec<GateMatrixRow>,
}

#[derive(Debug, Clone, Default)]
pub struct CompletionGateInputs {
    pub sdk_default: bool,
    pub project_router: bool,
    pub linux_vm: bool,
    pub fleet_spawn: bool,
    pub webhook: bool,
    pub rag: bool,
    pub vision_eval: bool,
    pub mcp_parity: bool,
    pub todo_discipline: bool,
    pub ralph_loop: bool,
    pub vibe_skill: bool,
}

fn row(requirement: &str, phase: &str, ok: bool, detail: impl Into<String>) -> GateMatrixRow {
    GateMatrixRow {
        requirement: requirement.to_string(),
        phase: phase.to_string(),
        status: if ok {
            GateStatus::Pass
        } else {
            GateStatus::Fail
        },
        detail: detail.into(),
    }
}

/// Evaluate the full user-ask matrix vs OpenHands/OmO/OpenClaw/Hermes combined.
#[must_use]
pub fn evaluate_completion_gate(inputs: &CompletionGateInputs) -> CompletionGateReport {
    let matrix = vec![
        row(
            "SDK-default Agent (not Rust subagent)",
            "phase-1",
            inputs.sdk_default,
            if inputs.sdk_default {
                "Agent tool delegates to TypeScript SDK by default"
            } else {
                "CLAW_AGENT_SDK still opt-in"
            },
        ),
        row(
            "Auto project route/create (user never sets up)",
            "phase-vm",
            inputs.project_router,
            "project_router.rs wired",
        ),
        row(
            "Linux coding VM owns execution",
            "phase-vm",
            inputs.linux_vm,
            "claw vm + CLAW_VM marker",
        ),
        row(
            "20-subagent fleet (tmux/worktree)",
            "phase-2",
            inputs.fleet_spawn,
            "fleet_spawn.rs + FleetSpawn tool",
        ),
        row(
            "n8n webhook automation",
            "phase-3",
            inputs.webhook,
            "claw webhook serve",
        ),
        row(
            "RAG + vectorized docs",
            "phase-4",
            inputs.rag,
            "retrieve_context in main claw",
        ),
        row(
            "Vision + IF/chess Python eval",
            "phase-5",
            inputs.vision_eval,
            "claw_eval + revision hooks",
        ),
        row(
            "Top MCP/skills/browser integrated",
            "phase-6",
            inputs.mcp_parity,
            "dynamic MCP launcher + ecosystem starter",
        ),
        row(
            "TodoWrite + note tools enforced",
            "phase-1b",
            inputs.todo_discipline,
            ".claw/todos.json persistence",
        ),
        row(
            "RALPH prd/progress/verify loop",
            "phase-ralph",
            inputs.ralph_loop,
            "prd.json + progress.txt",
        ),
        row(
            "Psychology vibe-coder skill",
            "phase-1",
            inputs.vibe_skill,
            "vibe-coder-orchestrator SKILL.md",
        ),
    ];
    let ready = matrix.iter().all(|r| r.status == GateStatus::Pass);
    CompletionGateReport { ready, matrix }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_pass_when_inputs_true() {
        let report = evaluate_completion_gate(&CompletionGateInputs {
            sdk_default: true,
            project_router: true,
            linux_vm: true,
            fleet_spawn: true,
            webhook: true,
            rag: true,
            vision_eval: true,
            mcp_parity: true,
            todo_discipline: true,
            ralph_loop: true,
            vibe_skill: true,
        });
        assert!(report.ready);
        assert_eq!(report.matrix.len(), 11);
    }
}
