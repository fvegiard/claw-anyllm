# AGENTS.md

## Cursor Cloud specific instructions

### What this repo is
The product is the Rust CLI `claw` (a Claude-CLI-style, plugin-shaped agent
harness that drives any LLM). The canonical workspace is `rust/` (a Cargo
workspace of crates under `rust/crates/`). The top-level `src/` and `tests/`
directories are legacy Python parity references, and `rust/rust/` is a duplicate
snapshot — build and test only from `rust/`.

### Toolchain (important)
- The committed `rust/Cargo.lock` pins dependencies that need a recent stable
  toolchain: some deps require `rustc >= 1.88`, and `runtime` uses the
  `const Duration::from_hours` API that only became usable on newer stable
  (the pre-installed `1.83` and even `1.89` fail to build). Use a current
  `stable` toolchain (verified working on `1.96.1`). The startup update script
  runs `rustup default stable`, so the environment is already on stable.

### Build / test / run (all from `rust/`)
- Build: `cargo build --workspace`
- Tests: `cargo test --workspace --lib` runs the full unit-test suite (~1034
  tests, all passing). Note: the integration test file
  `crates/rusty-claude-cli/tests/output_format_contract.rs` is out of sync with
  the current code and fails to compile (missing `think_mode` field / function
  arity), which makes a bare `cargo test --workspace` fail at build. This is a
  pre-existing repo issue, not an environment problem.
- Lint: `cargo clippy --workspace --all-targets -- -D warnings` and format check
  `../scripts/fmt.sh --check` both run but report pre-existing violations
  (e.g. `clippy::unnecessary_cast`, `clippy::extend_with_drain`, and unformatted
  code) that exist across all buildable toolchains. Standard commands are in
  `CONTRIBUTING.md` and `rust/CLAUDE.md`.
- Run the app: `./target/debug/claw --help` (see `README.md`). `claw doctor`,
  `claw status`, and `claw version` work with no credentials.

### Running the agent loop offline (no real API key)
Real prompts need `ANTHROPIC_API_KEY` and hit `https://api.anthropic.com`. To
exercise the full conversation/tool loop with no network, use the bundled mock
Anthropic backend:
1. `./target/debug/mock-anthropic-service --bind 127.0.0.1:8787` (prints its
   base URL; keep it running).
2. `export ANTHROPIC_BASE_URL="http://127.0.0.1:8787"` and any dummy
   `export ANTHROPIC_API_KEY="sk-ant-test-dummy-key"`.
3. Prompts MUST contain a scenario token or the mock rejects them, e.g.
   `./target/debug/claw --output-format text prompt "PARITY_SCENARIO:streaming_text say hello"`.
   Valid scenarios are defined in `crates/mock-anthropic-service/src/lib.rs`
   (e.g. `streaming_text`, `read_file_roundtrip`, `bash_stdout_roundtrip`).

### Gotcha
- Running `claw` with a shell-executing tool (e.g. `--allowedTools bash`)
  through the non-interactive command wrapper can abort due to nested
  sandbox/subprocess handling. Running it inside an interactive terminal
  (e.g. a tmux/desktop shell) works fine — prefer that for tool-execution demos.
