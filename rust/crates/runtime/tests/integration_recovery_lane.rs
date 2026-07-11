//! Integration tests for B1 (inspect lane): wire `attempt_recovery` and
//! `LaneEventBuilder` into the worker boot path.
//!
//! These tests guard against the regression where `WorkerRegistry` had
//! the recovery/lane-event building blocks but no method to invoke them
//! against a real `Worker`. The test exercises the full path:
//!
//!   1. Create a worker
//!   2. Drive it to a `Provider` failure
//!   3. Call `WorkerRegistry::attempt_recovery_for` and assert the result
//!   4. Call `WorkerRegistry::recovery_ledger` and assert the ledger entry
//!   5. Call `WorkerRegistry::lane_event_for_failure` and assert the event

use runtime::worker_boot::{WorkerFailureKind, WorkerRegistry, WorkerStatus};
use runtime::{EventProvenance, LaneEventName, LaneEventStatus, LaneFailureClass};
use runtime::{RecoveryEvent, RecoveryResult};

#[test]
fn attempt_recovery_for_provider_failure_yields_recovered() {
    // given — a worker that reaches a Provider failure
    let registry = WorkerRegistry::new();
    let worker = registry.create("/tmp/b1-recovery-test", &[], true);

    // Drive to ready
    registry
        .observe(&worker.worker_id, "Ready for your input\n>")
        .expect("ready observe should succeed");
    registry
        .send_prompt(&worker.worker_id, Some("Run analysis"), None)
        .expect("send_prompt should succeed");

    // Drive to completion with provider failure (finish="unknown", tokens=0)
    let failed = registry
        .observe_completion(&worker.worker_id, "unknown", 0)
        .expect("observe_completion should succeed");
    assert_eq!(failed.status, WorkerStatus::Failed);
    assert_eq!(
        failed.last_error.as_ref().expect("failure recorded").kind,
        WorkerFailureKind::Provider
    );

    // when — invoke the recovery helper
    let result = registry
        .attempt_recovery_for(&failed.worker_id)
        .expect("attempt_recovery_for should succeed");

    // then — recovery should succeed for the ProviderFailure recipe
    assert!(
        matches!(result, RecoveryResult::Recovered { steps_taken: 1 }),
        "expected Recovered {{ steps_taken: 1 }}, got {result:?}"
    );

    // and the ledger should have one entry for ProviderFailure
    let ledger = registry
        .recovery_ledger(&failed.worker_id)
        .expect("recovery_ledger should succeed");
    assert_eq!(ledger.len(), 1, "ledger should have exactly one entry");
    let entry = &ledger[0];
    assert_eq!(
        entry.trigger,
        runtime::recovery_recipes::FailureScenario::ProviderFailure
    );
    assert_eq!(entry.attempt_count, 1);
    assert!(matches!(
        entry.state,
        runtime::recovery_recipes::RecoveryAttemptState::Succeeded
    ));
}

#[test]
fn attempt_recovery_for_trust_gate_failure_yields_recovered() {
    // given — a worker that hits a TrustGate failure
    let registry = WorkerRegistry::new();
    let worker = registry.create("/tmp/b1-trust-test", &[], false);
    let observed = registry
        .observe(
            &worker.worker_id,
            "Do you trust the files in this folder? (y/n)",
        )
        .expect("observe should succeed");
    assert_eq!(observed.status, WorkerStatus::TrustRequired);
    assert_eq!(
        observed.last_error.as_ref().expect("failure recorded").kind,
        WorkerFailureKind::TrustGate
    );

    // when
    let result = registry
        .attempt_recovery_for(&worker.worker_id)
        .expect("attempt_recovery_for should succeed");

    // then
    assert!(
        matches!(result, RecoveryResult::Recovered { .. }),
        "TrustGate should recover, got {result:?}"
    );
    let ledger = registry
        .recovery_ledger(&worker.worker_id)
        .expect("recovery_ledger should succeed");
    assert_eq!(ledger.len(), 1);
    assert_eq!(
        ledger[0].trigger,
        runtime::recovery_recipes::FailureScenario::TrustPromptUnresolved
    );
}

#[test]
fn attempt_recovery_for_returns_err_when_worker_has_no_failure() {
    // given — a freshly-created worker with no recorded failure
    let registry = WorkerRegistry::new();
    let worker = registry.create("/tmp/b1-no-failure", &[], false);

    // when
    let result = registry.attempt_recovery_for(&worker.worker_id);

    // then
    let err = result.expect_err("should error when no failure recorded");
    assert!(
        err.contains("no current failure"),
        "error should mention missing failure, got: {err}"
    );
}

#[test]
fn attempt_recovery_for_returns_err_for_unknown_worker() {
    // given
    let registry = WorkerRegistry::new();

    // when
    let result = registry.attempt_recovery_for("worker_does_not_exist");

    // then
    let err = result.expect_err("should error for unknown worker");
    assert!(err.contains("worker not found"), "got: {err}");
}

#[test]
fn lane_event_for_failure_returns_typed_lane_failed_event() {
    // given — a worker with a TrustGate failure
    let registry = WorkerRegistry::new();
    let worker = registry.create("/tmp/b1-lane-event", &[], false);
    let observed = registry
        .observe(
            &worker.worker_id,
            "Do you trust the files in this folder? (y/n)",
        )
        .expect("observe should succeed");
    assert_eq!(
        observed.last_error.as_ref().expect("failure").kind,
        WorkerFailureKind::TrustGate
    );

    // when
    let event = registry
        .lane_event_for_failure(&worker.worker_id, EventProvenance::LiveLane)
        .expect("lane_event_for_failure should succeed");

    // then
    assert_eq!(event.event, LaneEventName::Failed);
    assert_eq!(event.status, LaneEventStatus::Failed);
    assert_eq!(event.failure_class, Some(LaneFailureClass::TrustGate));
    assert!(event.detail.is_some(), "detail should be populated");
    assert!(
        event.detail.as_ref().expect("detail").contains("trust"),
        "detail should mention trust, got: {:?}",
        event.detail
    );
}

#[test]
fn lane_event_for_failure_maps_provider_failure_to_infra_class() {
    // given — a worker with a Provider failure
    let registry = WorkerRegistry::new();
    let worker = registry.create("/tmp/b1-lane-event-provider", &[], true);
    registry
        .observe(&worker.worker_id, "Ready for your input\n>")
        .expect("ready observe should succeed");
    registry
        .send_prompt(&worker.worker_id, Some("Run"), None)
        .expect("send_prompt should succeed");
    let failed = registry
        .observe_completion(&worker.worker_id, "unknown", 0)
        .expect("completion should succeed");
    assert_eq!(
        failed.last_error.as_ref().expect("failure").kind,
        WorkerFailureKind::Provider
    );

    // when
    let event = registry
        .lane_event_for_failure(&failed.worker_id, EventProvenance::LiveLane)
        .expect("lane_event_for_failure should succeed");

    // then
    assert_eq!(event.failure_class, Some(LaneFailureClass::Infra));
}

#[test]
fn recovery_events_logged_in_context_after_attempt() {
    // given
    let registry = WorkerRegistry::new();
    let worker = registry.create("/tmp/b1-recovery-events", &[], false);
    registry
        .observe(
            &worker.worker_id,
            "Do you trust the files in this folder? (y/n)",
        )
        .expect("observe should succeed");

    // when
    registry
        .attempt_recovery_for(&worker.worker_id)
        .expect("attempt should succeed");

    // then — read the worker's recovery context events directly
    let worker_after = registry
        .get(&worker.worker_id)
        .expect("worker should exist");
    let events = worker_after.recovery_context.events();
    assert!(
        events.iter().any(|e| matches!(
            e,
            RecoveryEvent::RecoveryAttempted {
                result: RecoveryResult::Recovered { .. },
                ..
            }
        )),
        "recovery context should log a RecoveryAttempted event with Recovered result; got: {events:?}"
    );
}
