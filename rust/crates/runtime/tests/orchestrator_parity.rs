//! Orchestrator parity harness smoke tests.

#[test]
fn hermes_mcp_form_uses_mcp_servers() {
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let starter =
        std::fs::read_to_string(manifest.join("../../../examples/ecosystem-mcp-starter.json"))
            .expect("ecosystem starter");
    assert!(starter.contains("mcpServers"));
}

#[test]
fn n8n_webhook_template_has_orchestrate_path() {
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workflow =
        std::fs::read_to_string(manifest.join("../../../examples/n8n-claw-webhook-workflow.json"))
            .expect("n8n workflow");
    assert!(workflow.contains("/v1/orchestrate"));
}

#[test]
fn completion_gate_matrix_has_eleven_rows() {
    let report = runtime::completion_gate::evaluate_completion_gate(
        &runtime::completion_gate::CompletionGateInputs::default(),
    );
    assert_eq!(report.matrix.len(), 11);
    assert!(!report.ready);
}

#[test]
fn project_router_scaffolds_node_project() {
    let home = std::env::temp_dir().join(format!("claw-parity-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("temp");
    let root = home.join("projects");
    let result = runtime::project_router::route_project(
        &home,
        &runtime::project_router::ProjectRouteRequest {
            intent: String::from("build todo app"),
            repo_url: None,
            project_name: Some(String::from("parity-todo")),
            stack: Some(String::from("node")),
            workspace_root: Some(root.display().to_string()),
        },
    )
    .expect("route");
    assert_eq!(
        result.action,
        runtime::project_router::ProjectRouteAction::Scaffold
    );
    let _ = std::fs::remove_dir_all(&home);
}
