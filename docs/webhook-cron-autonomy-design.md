# Autonomous Webhook + Cron + Auto-Resume Design

Status: draft scaffold. Grounded in the actual types already in this repo
(`runtime/src/team_cron_registry.rs`, `runtime/src/worker_boot.rs`,
`runtime/src/session_control.rs`, `runtime/src/remote.rs`) plus the existing
`examples/n8n-claw-webhook-workflow.json`.

## What already exists

- `CronRegistry` / `TeamRegistry` (`team_cron_registry.rs`): in-memory CRUD only.
-   `CronEntry` has `schedule`, `prompt`, `enabled`, `last_run_at`, `run_count`,
-     and `record_run()` — but nothing ever calls `record_run` on a timer. There is
-   no scheduler loop today.
-   - `WorkerRegistry` (`worker_boot.rs`): a full lifecycle state machine
    -   (Spawning -> TrustRequired/ToolPermissionRequired -> ReadyForPrompt -> Running
    -     -> Finished/Failed) with `restart()`, `send_prompt()` (which replays
    -   `replay_prompt` automatically), `observe_startup_timeout()` (classifies stalls),
    -     and `emit_state_file()` which writes `.claw/worker-state.json` for external
    -   pollers. This is 90% of "auto resume" already built at the process layer.
    -   - `SessionStore` (`session_control.rs`): per-workspace persisted conversation
        -   history with `/resume latest` alias resolution. This is "auto resume" at the
        -     conversation layer.
        - - `remote.rs`: env-driven remote proxy/session bootstrap (CCR token, upstream
          -   proxy). Shows the established pattern for env-configured remote wiring.
          -   - Webhooks today are 100% external: `examples/n8n-claw-webhook-workflow.json`
              -   has n8n receive a POST and shell out to
              -     `examples/agent-sdk-orchestrator` via `npm run orchestrate:json`. There is no
              -   native HTTP listener in the Rust binary.
           
              -   ## Gaps to close
           
              -   1. No native webhook server in `claw` itself (only reachable via external n8n).
                  2. 2. `CronRegistry` entries are never actually executed on a schedule.
                     3. 3. No supervisor loop ties `WorkerRegistry.restart()` + `observe_startup_timeout()`
                        4.    together into an automatic watchdog; both exist but nothing calls them
                        5.   periodically today.
                        6.   4. No "wait timer" primitive for agent-initiated delays (e.g. "check back in
                             5.    10 minutes") distinct from the cron registry (recurring) case.
                          
                             6.## Proposed additions (new crate: `rust/crates/scheduler`)

                             ### 1. Webhook server (`scheduler/src/webhook.rs`)

                             Native `axum` HTTP listener, replacing the n8n shell-out with a first-class
                             `claw serve --webhook 0.0.0.0:4173` mode. n8n / GitHub / Slack can still point
                             at it directly; this just removes the Node hop for the default path.

                             ```rust
                             use axum::{routing::post, Json, Router, extract::State};
                             use serde::Deserialize;

                             #[derive(Deserialize)]
                             pub struct WebhookPayload {
                                 pub prompt: Option<String>,
                                 pub message: Option<String>,
                                 pub agent: Option<String>,
                                 pub session: Option<String>, // "latest" to resume, or explicit session id
                             }

                             pub async fn handle_webhook(
                                 State(state): State<SchedulerState>,
                                 Json(payload): Json<WebhookPayload>,
                             ) -> Json<serde_json::Value> {
                                 let text = payload.prompt.or(payload.message).unwrap_or_default();
                                 let session_ref = payload.session.as_deref().unwrap_or("latest");

                                 // Reuse SessionStore::resolve_reference_excluding + WorkerRegistry as-is.
                                 let outcome = state.dispatch_prompt(session_ref, &text, payload.agent).await;
                                 Json(serde_json::json!({ "status": outcome.status, "worker_id": outcome.worker_id }))
                             }

                             pub fn router(state: SchedulerState) -> Router {
                                 Router::new()
                                     .route("/webhook/claw-orchestrate", post(handle_webhook))
                                     .with_state(state)
                             }
                             ```

                             Auth: require a shared-secret header (`X-Claw-Webhook-Secret`) checked against
                             an env var (`CLAW_WEBHOOK_SECRET`), following the same env-driven pattern as
                             `remote.rs`. Reject unsigned requests with 401 before touching any registry.

                             ### 2. Cron scheduler loop (`scheduler/src/cron_tick.rs`)

                             Actually executes `CronRegistry` entries. Uses the `cron` crate to parse the
                             existing `schedule: String` field (already 5/6-field cron syntax per the
                             tests, e.g. `"0 * * * *"`).

                             ```rust
                             use std::time::Duration;
                             use tokio::time::sleep;

                             pub async fn run_cron_loop(cron_registry: CronRegistry, dispatcher: PromptDispatcher) {
                                 loop {
                                     let due = cron_registry
                                         .list(true) // enabled only
                                         .into_iter()
                                         .filter(|entry| is_due(entry));

                                     for entry in due {
                                         let result = dispatcher.dispatch_prompt("latest", &entry.prompt, None).await;
                                         cron_registry.record_run(&entry.cron_id).ok();
                                         if result.is_err() {
                                             // record_run still happens; failure surfaces via worker_boot's
                                             // existing WorkerFailure/last_error, not a separate cron error path.
                                         }
                                     }
                                     sleep(Duration::from_secs(30)).await; // tick interval, configurable
                                 }
                             }

                             fn is_due(entry: &CronEntry) -> bool {
                                 let schedule: cron::Schedule = entry.schedule.parse().expect("validated at create()");
                                 let now = chrono::Utc::now();
                                 match entry.last_run_at {
                                     None => true,
                                     Some(last) => schedule
                                         .after(&chrono::DateTime::from_timestamp(last as i64, 0).unwrap())
                                         .next()
                                         .is_some_and(|next| next <= now),
                                 }
                             }
                             ```

                             Validate the cron expression inside `CronRegistry::create()` (currently it
                             accepts any string) so bad schedules fail fast at creation, not silently at
                             tick time.

                             ### 3. Wait-timer worker (`scheduler/src/wait_timer.rs`)

                             For one-off delayed resumption ("come back to this in 10 minutes") as opposed
                             to `CronRegistry`'s recurring schedules. Distinct because it is single-shot
                             and tied to a specific worker/session, not a standing registry entry.

                             ```rust
                             pub struct WaitTimer {
                                 pub worker_id: String,
                                 pub resume_at: u64, // unix seconds
                                 pub resume_prompt: Option<String>, // falls back to replay_prompt if None
                             }

                             pub async fn run_wait_timers(worker_registry: WorkerRegistry, timers: Arc<Mutex<Vec<WaitTimer>>>) {
                                 loop {
                                     let now = now_secs();
                                     let mut guard = timers.lock().expect("wait timer lock poisoned");
                                     let (due, pending): (Vec<_>, Vec<_>) = guard.drain(..).partition(|t| t.resume_at <= now);
                                     *guard = pending;
                                     drop(guard);

                                     for timer in due {
                                         let _ = worker_registry.send_prompt(
                                             &timer.worker_id,
                                             timer.resume_prompt.as_deref(),
                                             None,
                                         );
                                     }
                                     sleep(Duration::from_secs(5)).await;
                                 }
                             }
                             ```

                             Exposed to the LLM as a tool (`wait_then_resume(seconds, prompt)`) so the
                             agent itself can decide to defer, mirroring how `TaskToolSet` already exposes
                             delegation.

                             ### 4. Auto-resume watchdog (`scheduler/src/watchdog.rs`)

                             This is the piece that actually wires `worker_boot.rs`'s existing
                             `observe_startup_timeout()` and `restart()` into an automatic loop instead of
                             requiring a human or external poller to call them.

                             ```rust
                             pub async fn run_watchdog(worker_registry: WorkerRegistry, stall_timeout: Duration) {
                                 loop {
                                     for worker in worker_registry.list_active() { // new helper: non-terminal statuses
                                         let elapsed = now_secs().saturating_sub(worker.updated_at);
                                         if elapsed > stall_timeout.as_secs() {
                                             let transport_healthy = probe_transport(&worker).await;
                                             let mcp_healthy = probe_mcp(&worker).await;
                                             let observed = worker_registry.observe_startup_timeout(
                                                 &worker.worker_id, "watchdog-probe", transport_healthy, mcp_healthy,
                                             );
                                             if let Ok(w) = observed {
                                                 if w.replay_prompt.is_some() || w.status == WorkerStatus::Failed {
                                                     let _ = worker_registry.restart(&worker.worker_id);
                                                     // Supervisor process then respawns the underlying CLI
                                                     // subprocess and calls send_prompt() with replay_prompt.
                                                 }
                                             }
                                         }
                                     }
                                     sleep(Duration::from_secs(10)).await;
                                 }
                             }
                             ```

                             `list_active()` is a small new method on `WorkerRegistry` filtering out
                             `Finished`/terminal states — everything else (the state enum, event log,
                             `.claw/worker-state.json` emission) is reused unchanged.

                             ## Wiring it together

                             `claw serve` becomes the long-running autonomous mode: it starts the axum
                             webhook router, the cron tick loop, the wait-timer loop, and the watchdog loop
                             as four `tokio::spawn`ed tasks sharing one `WorkerRegistry` + `CronRegistry` +
                             `SessionStore`. A webhook or cron firing both go through the same
                             `PromptDispatcher::dispatch_prompt(session_ref, text, agent)` entry point, so
                             there is exactly one code path for "get a prompt into a session," whether it
                             came from GitHub, n8n, a timer, or a human.

                             ## Open questions before implementing

                             - Which cron-parsing crate to standardize on (`cron` vs `saffron`) — depends
                             -   on whether 6-field (with seconds) syntax is needed.
                             -   - Where `scheduler` sits in the workspace: standalone crate vs folder inside
                                 -   `runtime`, given `runtime` already owns `worker_boot`/`team_cron_registry`.
                                 -   - Webhook auth: shared secret env var (simplest, matches `remote.rs` pattern)
                                     -   vs HMAC signature verification (needed if GitHub webhooks are a direct
                                     -     target instead of going through n8n).
                                     - 
