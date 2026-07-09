# claw-anyllm

> A **plugin-shaped** agent harness. Drives any LLM — including
> [Mavis](https://github.com/fvegiard/autoselect-skill) — through a
> Claude-CLI-style REPL. Born from a re-derivation of
> [`ultraworkers/claw-code`](https://github.com/ultraworkers/claw-code), not a
> fork of its repo.

**TL;DR:**

- `cargo build --workspace` → runs `claw` against any LLM
- `--think` flag wraps the runtime in a step-by-step reasoning scaffold
- `OpenHands` tool delegates shell execution to a configurable HTTP backend
- **Plugin manifest v2** lets you register a new LLM backend (Mavis, Ollama,
  anything) by writing **one JSON file** — no Rust change required

---

## What is this?

A complete Rust workspace that implements a Claude-CLI-style agent harness
where **the LLM is a plugin**. The conversation loop, tool execution, and
permission model are all LLM-agnostic — the LLM is just another plugin entry
declared in a manifest file.

You can:

1. **Run it out of the box** against Anthropic Claude (set `ANTHROPIC_API_KEY`).
2. **Plug in a different LLM** (Mavis, Ollama, your OpenAI-compatible API) by
   dropping a `plugin.json` manifest. No recompile.
3. **Add new tools** by writing a single JSON declaration. The runtime discovers
   and registers them.
4. **Toggle "think mode"** with `--think` for step-by-step reasoning on hard
   tasks.

This makes the whole thing a **plugin-shaped agent harness**: the host is
fixed, the brain and the limbs are pluggable.

---

## The plugin contract

A plugin manifest is a JSON file at `.claude-plugin/plugin.json`. The v2
schema supports three arrays — all optional, all backwards-compatible with v1
manifests that omit them:

```json
{
  "schema_version": 2,
  "name": "my-plugin",
  "version": "0.1.0",
  "kind": "external",
  "providers": [
    {
      "id": "mavis",
      "display_name": "Mavis",
      "endpoint": "https://api.example.com/v1",
      "auth_env": "MAVIS_TOKEN",
      "default_model": "MiniMax-M3"
    }
  ],
  "tools": [
    {
      "name": "my_tool",
      "description": "Does something useful",
      "input_schema": { "type": "object", "properties": {} }
    }
  ],
  "hooks": [
    { "event": "PreToolUse", "command": "echo 'tool about to run'" }
  ]
}
```

Once a manifest is installed, the agent harness picks it up. See
[`docs/plugin-manifest.md`](./docs/plugin-manifest.md) for the full schema
and [`examples/`](./examples/) for runnable recipes.

---

## Built-in plugins

Three plugins ship with the harness:

| Plugin | What | How to use |
|--------|------|------------|
| **Think mode** | Wraps every reply in a "think first" scaffold. The runtime prepends a step-by-step reasoning directive and parses `<thinking>...</thinking>` blocks back out of the response. | `./claw --think prompt "..."` |
| **OpenHands** | A generic HTTP shell-exec backend. Sends `{"code": ..., "language": ...}` to a configurable endpoint. Two recipes: Bearer auth (default) or `X-Session-API-Key` (OpenHands Agent Server style). | `OPENHANDS_ENDPOINT=... ./claw prompt "..."` |
| **Plugin manifest v2** | The schema above. Already wired into the runtime's plugin loader. | Drop a manifest into `.claude-plugin/` |

---

## Build & run

```bash
# 1. Build
cd rust
cargo build --workspace

# 2. Set up auth (pick one)
export ANTHROPIC_API_KEY="sk-ant-..."          # Anthropic Claude
# or install a Mavis provider manifest:
#   cp examples/mavis-provider.json ~/.claude/plugins/mavis/plugin.json
#   export MAVIS_TOKEN="..."

# 3. Run
./target/debug/claw --help
./target/debug/claw --think prompt "say hello"
./target/debug/claw doctor
```

---

## Try the example plugins

```bash
# Look at the Mavis example
cat examples/mavis-provider.json

# Copy to the plugin loader's search path
mkdir -p ~/.claude/plugins/mavis
cp examples/mavis-provider.json ~/.claude/plugins/mavis/plugin.json

# Set auth and run
export MAVIS_TOKEN="your-token"
./target/debug/claw --provider mavis prompt "say hello"
```

---

## Project layout

```
.
├── README.md                     # this file
├── rust/                         # the canonical Rust workspace
│   ├── crates/
│   │   ├── runtime/              # conversation loop, system prompt
│   │   │   └── src/think_mode.rs        # think-mode plugin
│   │   ├── tools/                # tool registry + dispatch
│   │   │   └── src/openhands.rs         # openhands plugin
│   │   └── plugins/              # plugin manifest loader
│   │       └── src/manifest_v2.rs       # v2 manifest schema
│   └── ...
├── docs/
│   ├── plugin-manifest.md        # the v2 schema reference
│   ├── navigation-file-context.md
│   └── ...
├── examples/
│   ├── mavis-provider.json       # Mavis-style provider manifest
│   ├── ollama-provider.json      # Ollama-style provider manifest
│   └── README.md
└── LICENSE                       # MIT
```

---

## Test status

| Suite | Tests | Pass |
|-------|------:|-----:|
| `manifest_v2` | 9 | ✓ |
| `openhands`   | 12 | ✓ |
| `think_mode`  | 9 | ✓ |
| **Total new** | **30** | ✓ |

`cargo build --workspace` — clean, 0 warnings.

Two pre-existing tests fail in any sandbox without PowerShell or a real MCP
child process (`mcp_stdio::given_child_exits_after_discovery...`,
`tests::powershell_runs_via_stub_shell`). Unrelated to this work.

---

## Provenance

This repository is a re-derivation of
[`ultraworkers/claw-code`](https://github.com/ultraworkers/claw-code), an
upstream Rust port of Claude's CLI agent harness. The intent is not to track
upstream line-for-line; it's to demonstrate a plugin-shaped variant where the
LLM and the tool set are first-class plugin entries.

The original code that came from upstream is retained under the same MIT
license. See [LICENSE](./LICENSE) and the [upstream README](./UPSTREAM_README.md).

---

## License

MIT — same as the upstream project.

## See also

- [`fvegiard/autoselect-skill`](https://github.com/fvegiard/autoselect-skill) —
  companion skill that auto-selects which skills to load for a given turn.
- [`fvegiard/claw-code`](https://github.com/fvegiard/claw-code) — the
  upstream-derivation fork where this work was first done.
- [`ultraworkers/claw-code`](https://github.com/ultraworkers/claw-code) —
  the original.
