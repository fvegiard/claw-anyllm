# Examples

Runnable plugin manifests you can drop into `~/.claude/plugins/<name>/plugin.json`
to extend the harness.

| File | Backend | Auth | Default model |
|------|---------|------|---------------|
| `mavis-provider.json`  | Mavis (or any OpenAI-compatible API) | `MAVIS_TOKEN` env var | `MiniMax-M3` |
| `openai-provider.json` | OpenAI direct | `OPENAI_API_KEY` env var | `gpt-4o` |
| `ollama-provider.json` | Local Ollama | none | `llama3.1` |
| `ecosystem-mcp-starter.json` | Top GitHub MCP bundle (filesystem, GitHub, fetch, Playwright browser, Context7 docs, sequential-thinking) | per-server env vars | n/a (MCP config only) |
| `agent-sdk-orchestrator/` | TypeScript `@anthropic-ai/claude-agent-sdk` + Python 3D/IF evaluator | `ANTHROPIC_API_KEY`, optional `CLAW_AGENT_SDK=1` | nested SDK subagents |
| `n8n-claw-webhook-workflow.json` | n8n webhook → SDK orchestrator | n8n + Node 20+ | autonomous via webhook |

## Agent SDK orchestrator (not a normal subagent)

When `CLAW_AGENT_SDK=1`, claw's `Agent` tool delegates to the TypeScript SDK instead of the in-process Rust thread:

```bash
export CLAW_AGENT_SDK=1
export ANTHROPIC_API_KEY="..."
cd examples/agent-sdk-orchestrator && npm install
claw prompt "Use sdk:vibe-orchestrator — fix the header and verify UI with vision"
```

Python evaluator (`uv run` or `python3 -m claw_eval.cli`) scores moves on 3 axes (vision, correctness, safety) with explicit IF rules — chess-style best move.

## Ecosystem MCP starter

`ecosystem-mcp-starter.json` is a batteries-included `.claw/settings.json` fragment that wires the highest-signal MCP servers from the official MCP registry and community leaderboards:

- **filesystem** — workspace file ops (official MCP reference server; `.` = claw's current working directory)
- **github** — issues, PRs, repo search (inherits `GITHUB_PERSONAL_ACCESS_TOKEN` from your shell; export it before `claw`)
- **fetch** — web content for RAG-style retrieval
- **playwright** — headless browser automation (beats raw Puppeteer MCP token tax for agents)
- **context7** — live library/docs lookup (top community docs MCP)
- **sequential-thinking** — structured reasoning scaffold

```bash
mkdir -p .claw
cp ecosystem-mcp-starter.json .claw/settings.json
export GITHUB_PERSONAL_ACCESS_TOKEN="ghp_..."  # required for github MCP
# Set tokens as needed, then:
claw doctor --output-format json
claw mcp list --output-format json
```

Claw expands `${VAR}` in MCP `command`/`args`/`url` from the process environment (not VS Code `${env:VAR}` or `${workspaceFolder}`). Use `.` for the workspace root or `${PWD}` when you need an absolute path.

Claw uses flat `mcpServers`, not VS Code / Hermes nested `mcp.servers`. `claw doctor` warns if it detects the wrong shape.

## Install

```bash
# Pick one
mkdir -p ~/.claude/plugins/mavis
cp mavis-provider.json ~/.claude/plugins/mavis/plugin.json

mkdir -p ~/.claude/plugins/ollama
cp ollama-provider.json ~/.claude/plugins/ollama/plugin.json

# Set auth (where applicable)
export MAVIS_TOKEN="..."
# export OPENAI_API_KEY="..."

# Run with the new provider
./target/debug/claw --provider mavis prompt "say hello"
./target/debug/claw --provider ollama prompt "say hello"
```

## Roll your own

Copy any of these files, change the `id` and `endpoint` to point at your
backend, and drop it in. The runtime picks it up on the next launch.
