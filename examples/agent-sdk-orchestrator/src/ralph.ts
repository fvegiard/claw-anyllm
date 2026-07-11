import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

export interface PrdDocument {
  objective: string;
  criteria: string[];
  created_at: string;
}

export function prdPath(cwd: string): string {
  return path.join(cwd, "prd.json");
}

export function progressPath(cwd: string): string {
  return path.join(cwd, "progress.txt");
}

export function loadPrd(cwd: string, objective: string): PrdDocument {
  const file = prdPath(cwd);
  if (fs.existsSync(file)) {
    return JSON.parse(fs.readFileSync(file, "utf8")) as PrdDocument;
  }
  const doc: PrdDocument = {
    objective,
    criteria: [
      "Project routed or created without user setup",
      "SDK orchestrator completes with verify",
      "Todos completed or documented blocked",
    ],
    created_at: new Date().toISOString(),
  };
  fs.writeFileSync(file, JSON.stringify(doc, null, 2));
  return doc;
}

export function appendProgress(cwd: string, line: string): void {
  const file = progressPath(cwd);
  const entry = `[${new Date().toISOString()}] ${line}\n`;
  fs.appendFileSync(file, entry);
}

export function verifyBeforeComplete(cwd: string): { ok: boolean; reason?: string } {
  const doctor = spawnSync("claw", ["doctor", "--output-format", "json"], {
    cwd,
    encoding: "utf8",
  });
  if (doctor.status !== 0) {
    return { ok: false, reason: "claw doctor failed" };
  }
  try {
    const parsed = JSON.parse(doctor.stdout) as { has_failures?: boolean };
    if (parsed.has_failures) {
      return { ok: false, reason: "doctor has failures" };
    }
  } catch {
    return { ok: false, reason: "doctor output not JSON" };
  }
  return { ok: true };
}
