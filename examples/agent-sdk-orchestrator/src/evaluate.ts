import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

export interface CandidateAction {
  id: string;
  label: string;
  description?: string;
  vision_score?: number;
  correctness_score?: number;
  risk_score?: number;
  if_rules?: string[];
}

export interface DecisionResult {
  best: {
    action_id: string;
    label: string;
    total: number;
    rationale: string;
  };
  ranked: Array<{
    action_id: string;
    label: string;
    total: number;
    rationale: string;
  }>;
}

function pythonDir(): string {
  const here = path.dirname(fileURLToPath(import.meta.url));
  return path.join(here, "..", "python");
}

function runPythonEvaluator(payload: Record<string, unknown>): DecisionResult | null {
  const pythonDirPath = pythonDir();
  const attempts: Array<{ cmd: string; args: string[] }> = [
    { cmd: "uv", args: ["run", "python", "-m", "claw_eval.cli"] },
    { cmd: "python3", args: ["-m", "claw_eval.cli"] },
  ];

  for (const attempt of attempts) {
    const result = spawnSync(attempt.cmd, attempt.args, {
      cwd: pythonDirPath,
      input: JSON.stringify(payload),
      encoding: "utf8",
      env: { ...process.env, PYTHONPATH: pythonDirPath },
    });
    if (result.status === 0 && result.stdout?.trim()) {
      return JSON.parse(result.stdout) as DecisionResult;
    }
  }
  return null;
}

/**
 * Chess-style best move: score candidates on vision × correctness × safety + IF bonuses.
 */
export function pickBestMove(
  candidates: CandidateAction[],
  context: Record<string, unknown> = {},
): DecisionResult | null {
  if (process.env.CLAW_AUTO_EVAL === "0") {
    return null;
  }
  return runPythonEvaluator({
    mode: "decide",
    context,
    candidates,
  });
}

export function scoreScreenshot(imagePath: string): Record<string, unknown> | null {
  const pythonDirPath = pythonDir();
  const result = spawnSync(
    "python3",
    ["-m", "claw_eval.cli"],
    {
      cwd: pythonDirPath,
      input: JSON.stringify({ mode: "vision", image_path: imagePath }),
      encoding: "utf8",
      env: { ...process.env, PYTHONPATH: pythonDirPath },
    },
  );
  if (result.status !== 0 || !result.stdout?.trim()) {
    return null;
  }
  return JSON.parse(result.stdout) as Record<string, unknown>;
}
