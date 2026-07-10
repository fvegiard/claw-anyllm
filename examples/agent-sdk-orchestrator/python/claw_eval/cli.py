#!/usr/bin/env python3
"""CLI: stdin JSON → stdout JSON for orchestrator autonomous evaluation."""

from __future__ import annotations

import json
import sys

from .decision_engine import evaluate_candidates
from .vision_eval import best_vision_score


def main() -> None:
    raw = sys.stdin.read()
    payload = json.loads(raw) if raw.strip() else {}
    mode = payload.get("mode", "decide")

    if mode == "decide":
        result = evaluate_candidates(payload)
    elif mode == "vision":
        path = payload.get("image_path") or payload.get("path")
        if not path:
            raise SystemExit(json.dumps({"error": "missing image_path"}))
        result = best_vision_score(path)
    else:
        result = {"error": f"unknown mode: {mode}"}

    sys.stdout.write(json.dumps(result) + "\n")


if __name__ == "__main__":
    main()
