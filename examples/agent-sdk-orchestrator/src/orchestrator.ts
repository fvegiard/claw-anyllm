#!/usr/bin/env node
/**
 * CLI entry for claw → Agent SDK bridge.
 *
 * stdin JSON:
 *   { "prompt": "...", "description"?: "...", "agent"?: "vibe-orchestrator", "resume"?: "session-id", "cwd"?: "..." }
 *
 * stdout: single JSON result object
 */
import { query } from "@anthropic-ai/claude-agent-sdk";
import { ORCHESTRATOR_AGENTS, resolveAgentName } from "./agents.js";
import { pickBestMove } from "./evaluate.js";
import { revisionHooks } from "./hooks.js";
import { buildDefaultMcpServers } from "./mcp.js";

interface OrchestratorRequest {
  prompt: string;
  description?: string;
  agent?: string;
  resume?: string;
  cwd?: string;
  include_mcp?: boolean;
}

interface OrchestratorResult {
  status: "completed" | "error";
  result?: string;
  session_id?: string;
  agent_sdk: true;
  subagent?: string;
  error?: string;
}

function readStdin(): Promise<string> {
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = [];
    process.stdin.on("data", (chunk: Buffer) => chunks.push(chunk));
    process.stdin.on("end", () => resolve(Buffer.concat(chunks).toString("utf8")));
    process.stdin.on("error", reject);
  });
}

function parseArgs(): Partial<OrchestratorRequest> {
  const out: Partial<OrchestratorRequest> = {};
  const argv = process.argv.slice(2);
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--agent" && argv[i + 1]) {
      out.agent = argv[++i];
    } else if (arg === "--resume" && argv[i + 1]) {
      out.resume = argv[++i];
    } else if (arg === "--cwd" && argv[i + 1]) {
      out.cwd = argv[++i];
    } else if (arg === "--prompt" && argv[i + 1]) {
      out.prompt = argv[++i];
    } else if (!arg.startsWith("-") && !out.prompt) {
      out.prompt = arg;
    }
  }
  return out;
}

async function loadRequest(): Promise<OrchestratorRequest> {
  const fromArgs = parseArgs();
  if (!process.stdin.isTTY) {
    const raw = await readStdin();
    if (raw.trim()) {
      const parsed = JSON.parse(raw) as OrchestratorRequest;
      return { ...parsed, ...fromArgs, prompt: fromArgs.prompt ?? parsed.prompt };
    }
  }
  if (!fromArgs.prompt) {
    throw new Error("missing prompt (argv, --prompt, or stdin JSON)");
  }
  return fromArgs as OrchestratorRequest;
}

async function main(): Promise<void> {
  const req = await loadRequest();
  const cwd = req.cwd ?? process.cwd();
  const agentName = resolveAgentName(req.agent);
  const prompt = req.description
    ? `${req.description}\n\n${req.prompt}`
    : req.prompt;

  const delegatedPrompt = agentName
    ? `Use the ${agentName} agent. ${prompt}`
    : prompt;

  const preDecision = pickBestMove(
    [
      {
        id: "orchestrate-sdk",
        label: "Run full Agent SDK orchestration",
        correctness_score: 0.85,
        vision_score: prompt.toLowerCase().includes("ui") ? 0.9 : 0.6,
        risk_score: 0.25,
        if_rules: [
          "if ui_changed then vision-verify",
          "if tests_red then revision-reviewer",
        ],
      },
      {
        id: "direct-answer",
        label: "Answer without subagents",
        correctness_score: 0.4,
        vision_score: 0.3,
        risk_score: 0.1,
      },
    ],
    {
      ui_changed: /ui|screen|layout|button|color|vision|screenshot/i.test(prompt),
      tests_red: /test|fail|red|broken/i.test(prompt),
    },
  );

  let sessionId: string | undefined;
  let finalResult: string | undefined;
  let errorMessage: string | undefined;

  const mcpServers =
    req.include_mcp === false ? undefined : buildDefaultMcpServers(cwd);

  try {
    for await (const message of query({
      prompt: delegatedPrompt,
      options: {
        cwd,
        allowedTools: [
          "Read",
          "Write",
          "Edit",
          "Bash",
          "Grep",
          "Glob",
          "WebSearch",
          "WebFetch",
          "Agent",
          "Workflow",
          "TodoWrite",
        ],
        agents: ORCHESTRATOR_AGENTS,
        hooks: revisionHooks,
        mcpServers,
        resume: req.resume,
        systemPrompt: {
          type: "preset",
          preset: "claude_code",
          append: [
            "You orchestrate through the TypeScript Agent SDK.",
            "Prefer spawning specialized subagents over doing everything inline.",
            "Vision verification matters as much as code correctness for UI work.",
          ].join(" "),
        },
      },
    })) {
      const record = message as Record<string, unknown>;
      if (typeof record.session_id === "string") {
        sessionId = record.session_id;
      }
      if (typeof record.result === "string") {
        finalResult = record.result;
      }
    }
  } catch (error) {
    errorMessage = error instanceof Error ? error.message : String(error);
  }

  const output: OrchestratorResult = errorMessage
    ? {
        status: "error",
        agent_sdk: true,
        subagent: agentName,
        session_id: sessionId,
        error: errorMessage,
        result: finalResult,
      }
    : {
        status: "completed",
        agent_sdk: true,
        subagent: agentName,
        session_id: sessionId,
        result: [
          preDecision
            ? `AUTONOMOUS DECISION: ${preDecision.best.label} (score=${preDecision.best.total}) — ${preDecision.best.rationale}`
            : "",
          finalResult ?? "",
        ]
          .filter(Boolean)
          .join("\n\n"),
      };

  process.stdout.write(`${JSON.stringify(output)}\n`);
  process.exit(output.status === "error" ? 1 : 0);
}

main().catch((error: unknown) => {
  const message = error instanceof Error ? error.message : String(error);
  const output: OrchestratorResult = {
    status: "error",
    agent_sdk: true,
    error: message,
  };
  process.stdout.write(`${JSON.stringify(output)}\n`);
  process.exit(1);
});
