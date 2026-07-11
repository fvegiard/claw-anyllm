//! Orchestrator tools: project routing, fleet spawn, RAG retrieval.

use runtime::{
    fleet_spawn::{spawn_fleet_worker, FleetSpawnRequest},
    project_router::{route_project, ProjectRouteRequest},
};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Deserialize)]
pub struct ProjectRouteToolInput {
    pub intent: String,
    #[serde(default)]
    pub repo_url: Option<String>,
    #[serde(default)]
    pub project_name: Option<String>,
    #[serde(default)]
    pub stack: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct FleetSpawnToolInput {
    pub prompt: String,
    #[serde(default)]
    pub worker_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RetrieveContextInput {
    pub query: String,
    #[serde(default)]
    pub top_k: Option<u32>,
}

fn claw_home() -> std::path::PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join(".claw")
}

pub fn run_project_route(input: ProjectRouteToolInput) -> Result<String, String> {
    let result = route_project(
        &claw_home(),
        &ProjectRouteRequest {
            intent: input.intent,
            repo_url: input.repo_url,
            project_name: input.project_name,
            stack: input.stack,
            workspace_root: None,
        },
    )
    .map_err(|e| e.to_string())?;
    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}

pub fn run_fleet_spawn(input: FleetSpawnToolInput) -> Result<String, String> {
    let project_root = std::env::current_dir().map_err(|e| e.to_string())?;
    let result = spawn_fleet_worker(
        &claw_home(),
        &project_root,
        &FleetSpawnRequest {
            prompt: input.prompt,
            worker_id: input.worker_id,
            project_path: None,
        },
    )
    .map_err(|e| e.to_string())?;
    serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
}

pub fn run_retrieve_context(input: RetrieveContextInput) -> Result<String, String> {
    let base = std::env::var("RAG_BASE_URL")
        .or_else(|_| std::env::var("CLAW_RAG_BASE_URL"))
        .map_err(|_| {
            String::from("retrieve_context not configured (set RAG_BASE_URL or CLAW_RAG_BASE_URL)")
        })?;
    let url = format!("{}/v1/query", base.trim_end_matches('/'));
    let body = json!({
        "query": input.query,
        "top_k": input.top_k.unwrap_or(5),
    });
    let client = reqwest::blocking::Client::new();
    let response = client
        .post(&url)
        .json(&body)
        .send()
        .map_err(|e| format!("RAG request failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("RAG HTTP {}", response.status()));
    }
    let value: Value = response.json().map_err(|e| e.to_string())?;
    serde_json::to_string_pretty(&value).map_err(|e| e.to_string())
}
