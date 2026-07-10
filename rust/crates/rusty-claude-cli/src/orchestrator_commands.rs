//! VM, fleet, webhook, and completion-gate CLI handlers.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use runtime::{
    completion_gate::{evaluate_completion_gate, CompletionGateInputs},
    fleet_spawn::{list_fleet_workers, spawn_fleet_worker, FleetSpawnRequest},
    project_router::{route_project, ProjectRouteRequest},
    vm_runtime::{vm_exec, vm_status, vm_up, VmStatus},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::net::TcpListener;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrchestratorOutputFormat {
    Text,
    Json,
}

impl OrchestratorOutputFormat {
    fn from_cli(value: &str) -> Self {
        if value.eq_ignore_ascii_case("json") {
            Self::Json
        } else {
            Self::Text
        }
    }
}

fn claw_home() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".claw")
}

fn emit(output_format: OrchestratorOutputFormat, text: &str, json_value: Value) {
    match output_format {
        OrchestratorOutputFormat::Text => println!("{text}"),
        OrchestratorOutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json_value).unwrap_or_default()
            );
        }
    }
}

pub fn run_vm_up(
    output_format: OrchestratorOutputFormat,
) -> Result<(), Box<dyn std::error::Error>> {
    let result = vm_up();
    match result {
        Ok(message) => {
            emit(
                output_format,
                &message,
                json!({ "kind": "vm", "action": "up", "status": "ok", "message": message }),
            );
            Ok(())
        }
        Err(error) => {
            emit(
                output_format,
                &error,
                json!({ "kind": "vm", "action": "up", "status": "error", "message": error }),
            );
            Err(error.into())
        }
    }
}

pub fn run_vm_exec(
    command: &str,
    output_format: OrchestratorOutputFormat,
) -> Result<(), Box<dyn std::error::Error>> {
    let result = vm_exec(command)?;
    emit(
        output_format,
        &result,
        json!({ "kind": "vm", "action": "exec", "status": "ok", "stdout": result }),
    );
    Ok(())
}

pub fn run_vm_status(
    output_format: OrchestratorOutputFormat,
) -> Result<(), Box<dyn std::error::Error>> {
    let status: VmStatus = vm_status(&claw_home());
    let json_value = serde_json::to_value(&status)?;
    emit(
        output_format,
        &format!(
            "in_vm={} running={} workspace={} fleet={}",
            status.in_vm, status.running, status.workspace_root, status.fleet_count
        ),
        json!({ "kind": "vm", "action": "status", "status": "ok", "vm": status }),
    );
    let _ = json_value;
    Ok(())
}

pub fn run_fleet_list(
    output_format: OrchestratorOutputFormat,
) -> Result<(), Box<dyn std::error::Error>> {
    let workers = list_fleet_workers(&claw_home());
    emit(
        output_format,
        &format!("{} fleet workers", workers.len()),
        json!({ "kind": "fleet", "action": "list", "workers": workers }),
    );
    Ok(())
}

pub fn run_fleet_spawn(
    prompt: &str,
    output_format: OrchestratorOutputFormat,
) -> Result<(), Box<dyn std::error::Error>> {
    let project_root = std::env::current_dir()?;
    let result = spawn_fleet_worker(
        &claw_home(),
        &project_root,
        &FleetSpawnRequest {
            prompt: prompt.to_string(),
            worker_id: None,
            project_path: None,
        },
    )?;
    emit(
        output_format,
        &result.message,
        json!({ "kind": "fleet", "action": "spawn", "result": result }),
    );
    Ok(())
}

pub fn ensure_project_for_intent(
    intent: &str,
) -> Result<runtime::project_router::ProjectRouteResult, String> {
    route_project(
        &claw_home(),
        &ProjectRouteRequest {
            intent: intent.to_string(),
            repo_url: std::env::var("CLAW_REPO_URL").ok(),
            project_name: std::env::var("CLAW_PROJECT_NAME").ok(),
            stack: std::env::var("CLAW_STACK").ok(),
            workspace_root: None,
        },
    )
    .map_err(|e| e.to_string())
}

pub fn run_completion_gate(
    output_format: OrchestratorOutputFormat,
) -> Result<(), Box<dyn std::error::Error>> {
    let home = claw_home();
    let router = runtime::orchestrator_health::probe_project_router(&home).ok;
    let vm = runtime::orchestrator_health::probe_linux_vm(&home).ok;
    let fleet = runtime::orchestrator_health::probe_fleet_deps().ok;
    let python = runtime::orchestrator_health::probe_python_eval().ok;
    let todos = runtime::orchestrator_health::probe_todo_discipline(&home).ok;
    let report = evaluate_completion_gate(&CompletionGateInputs {
        sdk_default: tools::agent_sdk_bridge::sdk_default_enabled(),
        project_router: router,
        linux_vm: vm,
        fleet_spawn: fleet,
        webhook: true,
        rag: std::env::var("RAG_BASE_URL").is_ok(),
        vision_eval: python,
        mcp_parity: home.join("settings.json").is_file()
            || home.join("ecosystem-mcp-starter.json").is_file(),
        todo_discipline: todos,
        ralph_loop: std::env::current_dir()
            .map(|p| p.join("prd.json").is_file() || p.join("progress.txt").is_file())
            .unwrap_or(false),
        vibe_skill: home
            .join("skills")
            .join("vibe-coder-orchestrator")
            .join("SKILL.md")
            .is_file(),
    });
    emit(
        output_format,
        &format!("ready={}", report.ready),
        json!({ "kind": "completion_gate", "report": report }),
    );
    if !report.ready {
        return Err("completion gate not ready".into());
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct OrchestrateRequest {
    prompt: String,
    #[serde(default)]
    repo_url: Option<String>,
    #[serde(default)]
    project_name: Option<String>,
    #[serde(default)]
    stack: Option<String>,
}

#[derive(Clone)]
struct WebhookState {
    claw_home: PathBuf,
}

async fn health_handler() -> &'static str {
    "ok"
}

async fn orchestrate_handler(
    State(state): State<Arc<WebhookState>>,
    headers: HeaderMap,
    Json(req): Json<OrchestrateRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let expected = std::env::var("CLAW_WEBHOOK_TOKEN").map_err(|_| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            String::from("CLAW_WEBHOOK_TOKEN must be set"),
        )
    })?;
    if expected.is_empty() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            String::from("CLAW_WEBHOOK_TOKEN must not be empty"),
        ));
    }
    let provided = headers
        .get("x-claw-webhook-token")
        .and_then(|v| v.to_str().ok())
        .or_else(|| {
            headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .map(|v| v.strip_prefix("Bearer ").unwrap_or(v))
        })
        .unwrap_or_default();
    if provided != expected {
        return Err((
            StatusCode::UNAUTHORIZED,
            String::from("invalid webhook token"),
        ));
    }

    let route = route_project(
        &state.claw_home,
        &ProjectRouteRequest {
            intent: req.prompt.clone(),
            repo_url: req.repo_url,
            project_name: req.project_name,
            stack: req.stack,
            workspace_root: None,
        },
    )
    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    let bridge = tools::agent_sdk_bridge::AgentSdkBridgeInput {
        description: String::from("webhook orchestration"),
        prompt: req.prompt,
        subagent_type: Some(String::from("vibe-orchestrator")),
        name: None,
        model: None,
        cwd: Some(route.worktree.clone()),
    };
    let sdk_result = tools::agent_sdk_bridge::run_agent_sdk_bridge(&bridge)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(Json(json!({
        "project_route": route,
        "orchestrator": serde_json::from_str::<Value>(&sdk_result).unwrap_or(json!({ "raw": sdk_result })),
    })))
}

pub async fn run_webhook_serve(bind: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr: SocketAddr = bind
        .parse()
        .map_err(|e| format!("invalid bind address {bind}: {e}"))?;
    let state = Arc::new(WebhookState {
        claw_home: claw_home(),
    });
    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/v1/orchestrate", post(orchestrate_handler))
        .with_state(state);
    let listener = TcpListener::bind(addr).await?;
    eprintln!("claw webhook listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

pub fn parse_vm_args(args: &[String]) -> Result<(String, Vec<String>), String> {
    if args.is_empty() {
        return Err(String::from("usage: claw vm <up|exec|status>"));
    }
    Ok((args[0].clone(), args[1..].to_vec()))
}

pub fn parse_fleet_args(args: &[String]) -> Result<(String, Vec<String>), String> {
    if args.is_empty() {
        return Err(String::from("usage: claw fleet <list|spawn> [prompt]"));
    }
    Ok((args[0].clone(), args[1..].to_vec()))
}

pub fn parse_webhook_args(args: &[String]) -> Result<(String, String), String> {
    let action = args
        .first()
        .cloned()
        .unwrap_or_else(|| String::from("serve"));
    if action != "serve" {
        return Err(String::from("usage: claw webhook serve [--bind HOST:PORT]"));
    }
    let mut bind = String::from("127.0.0.1:8790");
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--bind" {
            if let Some(value) = args.get(i + 1) {
                bind = value.clone();
                i += 2;
                continue;
            }
            return Err(String::from("--bind requires a value"));
        }
        i += 1;
    }
    Ok((action, bind))
}
