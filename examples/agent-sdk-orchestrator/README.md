# Agent SDK orchestrator (TypeScript)

This is **not** a normal Rust-thread subagent. It runs the full
[`@anthropic-ai/claude-agent-sdk`](https://code.claude.com/docs/en/agent-sdk) with:

- Nested subagents (up to SDK depth limit)
- `Workflow` tool for large multi-agent runs
- `WebSearch` / `WebFetch` for GitHub + docs research
- Revision-loop hooks on tool failure
- Dynamic MCP (`npx` / `tsx` / `uvx`) for Playwright, filesystem, fetch, Context7

## Agents

| Name | Role |
|------|------|
| `vibe-orchestrator` | Decodes non-coder intent, delegates everything |
| `vision-looker` | Screenshot/UI verification — vision changes the game |
| `github-researcher` | Real GitHub examples via web search |
| `revision-reviewer` | Hook-loop brain on failures |
| `implementer` | Code + tests, can spawn nested agents |

## Install

```bash
cd examples/agent-sdk-orchestrator
npm install
export ANTHROPIC_API_KEY="..."
```

## Run standalone

```bash
npm run orchestrate -- --prompt "Make the login button blue and verify it looks right"
echo '{"prompt":"Fix the webhook","agent":"implementer"}' | npm run orchestrate:json
```

## From claw (recommended)

```bash
export CLAW_AGENT_SDK=1
export CLAW_AGENT_SDK_ORCHESTRATOR=/workspace/examples/agent-sdk-orchestrator

# Any Agent tool call now delegates to the SDK orchestrator:
claw prompt "Use agent vibe-orchestrator: ship a dark mode toggle and verify UI"
```

Or force per spawn:

```bash
claw prompt '...'  # with subagent_type sdk:vibe-orchestrator in Agent tool
```

## n8n webhook

Point an n8n Webhook node at a shell step:

```bash
echo "$json" | npm run orchestrate:json --prefix examples/agent-sdk-orchestrator
```

See `examples/n8n-claw-webhook-workflow.json`.

## Why SDK > Rust subagent

Rust `Agent` tool spawns an in-process thread with a reduced tool surface.
The TypeScript SDK gets the real Claude Code loop: isolated subagent context,
`parent_tool_use_id` tracking, Workflow orchestration, and filesystem agents in
`.claude/agents/`.
