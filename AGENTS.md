# AGENTS.md

## Cursor Cloud specific instructions

This repo contains two independent products:

- **Rust `claw` CLI workspace** (primary) — lives in `rust/`. Cargo workspace of
  ~11 crates; main binary is `claw` (crate `rusty-claude-cli`), plus a secondary
  `claw-analog` binary. See `rust/README.md` and root `USAGE.md` for the full
  command surface, and `CONTRIBUTING.md` for the standard build/test commands.
- **Python "porting workspace"** — lives in `src/` with tests in `tests/`.
  Standard-library only (no third-party packages, no `requirements.txt`).

### Toolchain (important, non-obvious)
- The Rust workspace requires the **`edition2024`** cargo feature (a transitive
  dependency, `clap`, uses it), so it needs **Rust ≥ 1.85**. The base image has
  historically shipped Rust 1.83, which fails early with
  `feature 'edition2024' is required`. The update script runs
  `rustup default stable` to pin a modern stable toolchain; if you ever see the
  `edition2024` error, re-run `rustup default stable`.
- There is a **duplicate nested copy** at `rust/rust/`. Ignore it — the canonical
  workspace is `/workspace/rust`. Always build/test from `/workspace/rust`.

### Build / run / test
- Rust (from `rust/`): `cargo build --workspace`; run the CLI with
  `./target/debug/claw --help` or `cargo run -p rusty-claude-cli -- ...`.
- Formatting is via `scripts/fmt.sh` (root) — **not** plain `cargo fmt` from a
  weird cwd (see `rust/CLAUDE.md`).
- Python (from repo root): `python3 -m unittest discover -s tests -v`; the CLI is
  `python3 -m src.main <subcommand>` (e.g. `summary`, `parity-audit`).

### End-to-end without a real API key
- The `claw` agent loop can be exercised offline against the bundled deterministic
  mock: start `cargo run -p mock-anthropic-service -- --bind 127.0.0.1:PORT`, then
  run `claw` with `ANTHROPIC_API_KEY=anything`, `ANTHROPIC_BASE_URL=http://127.0.0.1:PORT`
  and a prompt of the form `PARITY_SCENARIO:<name>` (names live in
  `rust/mock_parity_scenarios.json`, e.g. `streaming_text`, `write_file_allowed`).
- The scripted version is `rust/scripts/run_mock_parity_harness.sh`
  (`cargo test -p rusty-claude-cli --test mock_parity_harness`).
- Note: sending an **unknown** `PARITY_SCENARIO:` name makes the client retry ~9x
  with backoff (can take minutes) before failing — always use a real scenario name.

### Known pre-existing failures (NOT environment issues; do not "fix" as setup)
- `cargo test --workspace` fails to compile
  `crates/rusty-claude-cli/tests/output_format_contract.rs` (test drift: missing
  `think_mode` field on `CliAction` and a changed function arity). The rest of the
  suite is green — `cargo test --workspace --exclude rusty-claude-cli` passes 618
  tests with only the one item below failing.
- `mcp_stdio::tests::given_child_exits_after_discovery...` fails in sandboxes
  without a real MCP child process (documented in `rust/README.md`).
- `cargo clippy --workspace --all-targets -- -D warnings` fails on pre-existing
  lints (mostly in the `runtime` crate) that a modern clippy promotes to errors;
  `cargo fmt --check` also reports pre-existing drift.
