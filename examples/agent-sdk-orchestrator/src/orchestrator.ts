#!/usr/bin/env node
/**
 * CLI entry for claw → Agent SDK bridge.
 */
import { query } from "@anthropic-ai/claude-agent-sdk";
import { ORCHESTRATOR_AGENTS, resolveAgentName } from "./agents.js";
import { pickBestMove, type CandidateAction } from "./evaluate.js";
import { revisionHooks, captureUiIfNeeded, uiChangedInRepo } from "./hooks.js";
import { buildDefaultMcpServers } from "./mcp.js";
import { appendProgress, loadPrd, verifyBeforeComplete } from "./ralph.js";
import { loadTodos } from "./todos.js";

interface OrchestratorRequest {
  prompt: string;
  description?: string;
  agent?: string;
  resume?: string;
  cwd?: string;
  include_mcp?: boolean;
  candidates?: CandidateAction[];
}

interface OrchestratorResult {
  status: "completed" | "error" | "blocked";
  result?: string;
  session_id?: string;
  agent_sdk: true;
  subagent?: string;
  error?: string;
  project_route?: unknown;
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

function routeProject(prompt: string, cwd: string): unknown {
  return {
    note: "Project routing runs via claw prompt/webhook before orchestrator",
    intent: prompt.slice(0, 120),
    cwd,
  };
}

function defaultCandidates(prompt: string, cwd: string): CandidateAction[] {
  const ui = uiChangedInRepo(cwd) || /ui|screen|layout|button|color|vision|screenshot/i.test(prompt);
  return [
    {
      id: "orchestrate-sdk",
      label: "Run full Agent SDK orchestration",
      correctness_score: 0.85,
      vision_score: ui ? 0.9 : 0.6,
      risk_score: 0.25,
      if_rules: [
        "if ui_changed then vision-verify",
        "if tests_red then revision-reviewer",
      ],
    },
    {
      id: "vision-verify",
      label: "Vision-verify UI",
      correctness_score: 0.7,
      vision_score: 0.95,
      risk_score: 0.2,
      if_rules: ["if ui_changed then vision-verify"],
    },
    {
      id: "direct-answer",
      label: "Answer without subagents",
      correctness_score: 0.4,
      vision_score: 0.3,
      risk_score: 0.1,
    },
  ];
}

async function main(): Promise<void> {
  const req = await loadRequest();
  const cwd = req.cwd ?? process.cwd();
  const agentName = resolveAgentName(req.agent);
  const prompt = req.description
    ? `${req.description}\n\n${req.prompt}`
    : req.prompt;

  const prd = loadPrd(cwd, prompt);
  appendProgress(cwd, `iteration_start agent=${agentName ?? "default"}`);
  const projectRoute = routeProject(prompt, cwd);

  const candidates = req.candidates ?? defaultCandidates(prompt, cwd);
  const preDecision = pickBestMove(candidates, {
    ui_changed: uiChangedInRepo(cwd) || /ui/i.test(prompt),
    tests_red: /test|fail|red|broken/i.test(prompt),
  });

  const delegatedPrompt = agentName
    ? `Use the ${agentName} agent. Never ask the user to create a project or run setup. ${prompt}`
    : `Never ask the user to create a project or run setup. ${prompt}`;

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
          "NotebookEdit",
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
            "Maintain TodoWrite for every multi-step task.",
            "Prefer spawning specialized subagents over doing everything inline.",
            "Vision verification matters as much as code correctness for UI work.",
            `PRD criteria: ${prd.criteria.join("; ")}`,
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

  const shot = captureUiIfNeeded(cwd);
  if (shot) {
    appendProgress(cwd, `vision_capture ${shot}`);
  }

  const verify = verifyBeforeComplete(cwd);
  const todos = loadTodos(cwd);
  appendProgress(cwd, `iteration_end todos=${todos.todos.length} verify=${verify.ok}`);

  const blocked = !verify.ok && !errorMessage;

  const output: OrchestratorResult = errorMessage
    ? {
        status: "error",
        agent_sdk: true,
        subagent: agentName,
        session_id: sessionId,
        error: errorMessage,
        result: finalResult,
        project_route: projectRoute,
      }
    : {
        status: blocked ? "blocked" : "completed",
        agent_sdk: true,
        subagent: agentName,
        session_id: sessionId,
        project_route: projectRoute,
        result: [
          preDecision
            ? `AUTONOMOUS DECISION: ${preDecision.best.label} (score=${preDecision.best.total}) — ${preDecision.best.rationale}`
            : "",
          verify.ok ? "" : `VERIFY BLOCKED: ${verify.reason}`,
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
