# Plugin Manifest v2

Plugins declare their shape in a JSON manifest at `.claude-plugin/plugin.json`. The
v2 schema extends the v1 manifest by adding three optional arrays:

- `providers` — list of LLM backends this plugin registers
- `tools` — list of tools this plugin exposes
- `hooks` — list of hook handlers

All three arrays default to empty, so a v1-shaped manifest still parses unchanged.

## Schema

| Field | Type | Required | Notes |
|-------|------|----------|-------|
| `schema_version` | u32 | yes | Must be `2` |
| `name` | string | yes | Unique plugin name |
| `version` | string | yes | Semver recommended |
| `kind` | enum | yes | `builtin` / `bundled` / `external` |
| `description` | string | no | Free text |
| `providers` | array | no | Default `[]` |
| `tools` | array | no | Default `[]` |
| `hooks` | array | no | Default `[]` |

### `providers[i]`

| Field | Type | Required | Notes |
|-------|------|----------|-------|
| `id` | string | yes | e.g. `mavis`, `anthropic`, `ollama` |
| `display_name` | string | yes | UI-facing |
| `endpoint` | string | yes | Base URL of an OpenAI-compatible or Anthropic-compatible API |
| `auth_env` | string | no | Name of an environment variable holding the API key/token |
| `default_model` | string | yes | Model ID used when no override is given |

### `tools[i]`

| Field | Type | Required | Notes |
|-------|------|----------|-------|
| `name` | string | yes | Unique within the plugin |
| `description` | string | yes | What the tool does |
| `input_schema` | object | yes | JSON Schema for the tool's input |

### `hooks[i]`

| Field | Type | Required | Notes |
|-------|------|----------|-------|
| `event` | enum | yes | `PreToolUse` / `PostToolUse` / `PostToolUseFailure` |
| `command` | string | no | Shell command to run for this hook |

## Example — registering a Mavis (or compatible) provider

```json
{
  "schema_version": 2,
  "name": "mavis-bridge",
  "version": "0.1.0",
  "kind": "external",
  "description": "Bridge that registers Mavis (or compatible) as an LLM backend",
  "providers": [
    {
      "id": "mavis",
      "display_name": "Mavis",
      "endpoint": "https://api.example.com/v1",
      "auth_env": "MAVIS_TOKEN",
      "default_model": "MiniMax-M3"
    }
  ]
}
```

After installing this plugin, you can drive `claw` against a Mavis-compatible API:

```bash
export MAVIS_TOKEN="..."
claw --provider mavis prompt "say hello"
```

## Example — backwards-compat (v1-shaped manifest still parses)

```json
{
  "schema_version": 2,
  "name": "legacy-plugin",
  "version": "0.0.1",
  "kind": "bundled"
}
```

This parses cleanly: `providers`, `tools`, and `hooks` default to `[]`.

## Versioning

`schema_version` must be the exact integer this build expects (`SCHEMA_VERSION = 2`).
If we break the shape incompatibly in the future, this number will bump to `3` and v2
manifests will fail to parse — they'll need migration.

## Why this design

The whole point of v2 is to let plugin authors register new LLM backends without
having to ship Rust. A Mavis-style provider (or Ollama, or any OpenAI-compatible
API) can be wired in by writing one JSON file. No fork, no recompile.
