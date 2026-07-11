import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import type { HookCallback } from "@anthropic-ai/claude-agent-sdk";
import { appendProgress, loadPrd, verifyBeforeComplete } from "./ralph.js";
import { loadTodos, saveTodos, todosIncomplete } from "./todos.js";

const MAX_REVISION_ATTEMPTS = 5;
let revisionAttempts = 0;

function projectRoot(cwd: string): string {
  return cwd || process.cwd();
}

const postToolFailure: HookCallback = async (input) => {
  revisionAttempts += 1;
  const cwd = projectRoot((input as { cwd?: string }).cwd ?? process.cwd());
  appendProgress(cwd, `tool_failure attempt=${revisionAttempts} ${JSON.stringify(input)}`);
  if (revisionAttempts >= MAX_REVISION_ATTEMPTS) {
    return {
      decision: "block",
      reason: "Max revision attempts reached — escalate to human",
    };
  }
  return {
    decision: "continue",
    reason: "Delegate to revision-reviewer subagent",
  };
};

const stopHook: HookCallback = async (input) => {
  const cwd = projectRoot((input as { cwd?: string }).cwd ?? process.cwd());
  const todos = loadTodos(cwd);
  if (todosIncomplete(todos)) {
    appendProgress(cwd, "stop_blocked: incomplete todos");
    return {
      decision: "block",
      reason: "TodoWrite items still pending — complete or cancel todos before stopping",
    };
  }
  const verify = verifyBeforeComplete(cwd);
  appendProgress(cwd, `verify: ${JSON.stringify(verify)}`);
  if (!verify.ok) {
    return {
      decision: "block",
      reason: verify.reason ?? "RALPH verify failed",
    };
  }
  saveTodos(cwd, todos);
  return { decision: "continue" };
};

const sessionStart: HookCallback = async (input) => {
  const cwd = projectRoot((input as { cwd?: string }).cwd ?? process.cwd());
  const prd = loadPrd(cwd, "User orchestration session");
  appendProgress(cwd, `session_start prd_criteria=${prd.criteria.length}`);
  return {};
};

export const revisionHooks = {
  PostToolUseFailure: [{ hooks: [postToolFailure] }],
  Stop: [{ hooks: [stopHook] }],
  SessionStart: [{ hooks: [sessionStart] }],
};

export function uiChangedInRepo(cwd: string): boolean {
  const result = spawnSync("git", ["diff", "--name-only", "HEAD"], {
    cwd,
    encoding: "utf8",
  });
  if (result.status !== 0) {
    return false;
  }
  return /(\.tsx|\.jsx|\.css|\.html|\.vue|\.svelte)/i.test(result.stdout);
}

export function captureUiIfNeeded(cwd: string): string | null {
  if (!uiChangedInRepo(cwd)) {
    return null;
  }
  const shots = path.join(cwd, ".claw", "screenshots");
  fs.mkdirSync(shots, { recursive: true });
  const shot = path.join(shots, `capture-${Date.now()}.png`);
  const result = spawnSync(
    "npx",
    ["-y", "@playwright/mcp@latest", "screenshot", shot],
    { cwd, encoding: "utf8" },
  );
  return result.status === 0 ? shot : null;
}
