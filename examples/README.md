# Examples

Runnable plugin manifests you can drop into `~/.claude/plugins/<name>/plugin.json`
to extend the harness.

| File | Backend | Auth | Default model |
|------|---------|------|---------------|
| `mavis-provider.json`  | Mavis (or any OpenAI-compatible API) | `MAVIS_TOKEN` env var | `MiniMax-M3` |
| `openai-provider.json` | OpenAI direct | `OPENAI_API_KEY` env var | `gpt-4o` |
| `ollama-provider.json` | Local Ollama | none | `llama3.1` |
| `ecosystem-mcp-starter.json` | Top GitHub MCP bundle (filesystem, GitHub, fetch, Playwright browser, Context7 docs, sequential-thinking) | per-server env vars | n/a (MCP config only) |

## Ecosystem MCP starter

`ecosystem-mcp-starter.json` is a batteries-included `.claw/settings.json` fragment that wires the highest-signal MCP servers from the official MCP registry and community leaderboards:

- **filesystem** — workspace file ops (official MCP reference server)
- **github** — issues, PRs, repo search (`GITHUB_PERSONAL_ACCESS_TOKEN`)
- **fetch** — web content for RAG-style retrieval
- **playwright** — headless browser automation (beats raw Puppeteer MCP token tax for agents)
- **context7** — live library/docs lookup (top community docs MCP)
- **sequential-thinking** — structured reasoning scaffold

```bash
mkdir -p .claw
cp ecosystem-mcp-starter.json .claw/settings.json
# Set tokens as needed, then:
claw doctor --output-format json
claw mcp list --output-format json
```

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
