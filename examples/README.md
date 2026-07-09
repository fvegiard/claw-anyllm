# Examples

Runnable plugin manifests you can drop into `~/.claude/plugins/<name>/plugin.json`
to extend the harness.

| File | Backend | Auth | Default model |
|------|---------|------|---------------|
| `mavis-provider.json`  | Mavis (or any OpenAI-compatible API) | `MAVIS_TOKEN` env var | `MiniMax-M3` |
| `openai-provider.json` | OpenAI direct | `OPENAI_API_KEY` env var | `gpt-4o` |
| `ollama-provider.json` | Local Ollama | none | `llama3.1` |

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
