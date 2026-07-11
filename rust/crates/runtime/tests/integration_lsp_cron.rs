//! Integration tests for B2 (inspect lane): real LSP start/stop + Cron tick loop.
//!
//! These tests guard the new infrastructure that turns the previously-stub
//! `LspRegistry` and `CronRegistry` into something that can actually run.
//! Note: the LSP integration in this PR is **state-only** — process spawning
//! is deliberately left to a follow-up PR to avoid pulling in `lsp-types` or
//! `lsp-client`. The `tick` method on `CronRegistry` is real and synchronous;
//! the `spawn_tick_loop` wrappers exist but require a tokio runtime.

use runtime::lsp_client::{LspRegistry, LspServerStatus};
use runtime::team_cron_registry::CronRegistry;
use std::sync::{Arc, Mutex};

#[test]
fn lsp_registry_start_records_command_and_marks_starting() {
    // given
    let registry = LspRegistry::new();
    registry.register("rust", LspServerStatus::Disconnected, None, vec![]);

    // when
    let started = registry
        .start("rust", "rust-analyzer", vec!["--stdio".to_string()])
        .expect("start should succeed");

    // then
    assert_eq!(started.status, LspServerStatus::Starting);
    assert_eq!(started.lsp_command.as_deref(), Some("rust-analyzer"));
    assert_eq!(started.lsp_args, vec!["--stdio".to_string()]);
    assert!(registry.is_running("rust"));
}

#[test]
fn lsp_registry_start_with_empty_command_returns_err() {
    // given
    let registry = LspRegistry::new();
    registry.register("rust", LspServerStatus::Disconnected, None, vec![]);

    // when
    let result = registry.start("rust", "", vec![]);

    // then
    let err = result.expect_err("empty command should error");
    assert!(err.contains("empty command"), "got: {err}");
    assert!(!registry.is_running("rust"));
}

#[test]
fn lsp_registry_start_for_unknown_language_returns_err() {
    // given
    let registry = LspRegistry::new();

    // when
    let result = registry.start("cobol", "cobol-ls", vec![]);

    // then
    let err = result.expect_err("unknown language should error");
    assert!(err.contains("not found"), "got: {err}");
}

#[test]
fn lsp_registry_stop_clears_command_and_marks_disconnected() {
    // given
    let registry = LspRegistry::new();
    registry.register("rust", LspServerStatus::Disconnected, None, vec![]);
    registry
        .start("rust", "rust-analyzer", vec!["--stdio".to_string()])
        .expect("start should succeed");
    assert!(registry.is_running("rust"));

    // when
    let prev = registry.stop("rust").expect("stop should succeed");

    // then
    assert_eq!(prev.status, LspServerStatus::Starting);
    assert!(!registry.is_running("rust"));
    let after = registry.get("rust").expect("server should still exist");
    assert_eq!(after.status, LspServerStatus::Disconnected);
    assert!(after.lsp_command.is_none());
    assert!(after.lsp_args.is_empty());
}

#[test]
fn lsp_registry_is_running_is_false_for_unstarted_server() {
    // given
    let registry = LspRegistry::new();
    registry.register("rust", LspServerStatus::Disconnected, None, vec![]);

    // when / then
    assert!(!registry.is_running("rust"));
    assert!(!registry.is_running("nonexistent"));
}

#[test]
fn cron_registry_tick_invokes_callback_for_each_due_entry() {
    // given
    let registry = Arc::new(CronRegistry::new());
    registry.create("0 * * * *", "prompt A", None);
    registry.create("*/5 * * * *", "prompt B", None);
    let disabled = registry.create("0 0 * * *", "prompt C", None);
    registry
        .disable(&disabled.cron_id)
        .expect("disable should succeed");

    let fired = Arc::new(Mutex::new(Vec::<String>::new()));
    let fired_clone = Arc::clone(&fired);
    let on_due = move |entry: &runtime::team_cron_registry::CronEntry| {
        fired_clone.lock().expect("lock").push(entry.prompt.clone());
    };

    // when
    let count = registry.tick(on_due).expect("tick should succeed");

    // then
    assert_eq!(
        count, 2,
        "should fire the 2 enabled entries, not the disabled one"
    );
    let mut fired_prompts = fired.lock().expect("lock").clone();
    fired_prompts.sort();
    assert_eq!(
        fired_prompts,
        vec!["prompt A".to_string(), "prompt B".to_string()]
    );
}

#[test]
fn cron_registry_tick_increments_run_count() {
    // given
    let registry = Arc::new(CronRegistry::new());
    let entry = registry.create("*/5 * * * *", "tick me", None);

    // when
    let _ = registry.tick(|_| {}).expect("first tick");
    let _ = registry.tick(|_| {}).expect("second tick");

    // then
    let after = registry.get(&entry.cron_id).expect("entry should exist");
    assert_eq!(after.run_count, 2);
    assert!(after.last_run_at.is_some());
}

#[test]
fn cron_registry_tick_on_empty_registry_is_noop() {
    // given
    let registry = Arc::new(CronRegistry::new());

    // when
    let count = registry
        .tick(|_| panic!("callback should not be called"))
        .expect("tick should succeed");

    // then
    assert_eq!(count, 0);
}
