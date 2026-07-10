"""Chess-style best-move picker with explicit IF condition evaluation."""

from __future__ import annotations

import json
import re
from dataclasses import dataclass, field
from typing import Any


@dataclass(frozen=True)
class IfCondition:
    """Parsed IF branch: when <field> <op> <value> then prefer <action>."""

    field: str
    operator: str
    value: str | float | bool
    then_action: str
    raw: str


@dataclass
class CandidateAction:
    id: str
    label: str
    description: str = ""
    vision_score: float = 0.5
    correctness_score: float = 0.5
    risk_score: float = 0.5  # lower is safer; inverted in total
    tags: list[str] = field(default_factory=list)
    if_rules: list[str] = field(default_factory=list)


@dataclass(frozen=True)
class ScoredAction:
    action_id: str
    label: str
    total: float
    vision: float
    correctness: float
    safety: float
    if_bonus: float
    rationale: str


_IF_PATTERN = re.compile(
    r"if\s+(?P<field>\w+)\s*(?P<op>>=|<=|==|!=|>|<)\s*(?P<value>[^,\s]+)\s*then\s+(?P<then>.+)",
    re.IGNORECASE,
)


def parse_if_rule(rule: str) -> IfCondition | None:
    match = _IF_PATTERN.search(rule.strip())
    if not match:
        return None
    raw_value: str | float | bool = match.group("value")
    if raw_value.lower() in ("true", "false"):
        raw_value = raw_value.lower() == "true"
    else:
        try:
            raw_value = float(raw_value)
        except ValueError:
            pass
    return IfCondition(
        field=match.group("field"),
        operator=match.group("op"),
        value=raw_value,
        then_action=match.group("then").strip(),
        raw=rule.strip(),
    )


def _compare(left: Any, op: str, right: Any) -> bool:
    try:
        if op == "==":
            return left == right
        if op == "!=":
            return left != right
        if op == ">":
            return float(left) > float(right)
        if op == "<":
            return float(left) < float(right)
        if op == ">=":
            return float(left) >= float(right)
        if op == "<=":
            return float(left) <= float(right)
    except (TypeError, ValueError):
        return False
    return False


def if_bonus_for_action(action: CandidateAction, context: dict[str, Any]) -> float:
    bonus = 0.0
    for rule in action.if_rules:
        parsed = parse_if_rule(rule)
        if parsed is None:
            continue
        ctx_val = context.get(parsed.field)
        if ctx_val is None:
            continue
        if _compare(ctx_val, parsed.operator, parsed.value):
            if parsed.then_action.lower() in action.label.lower() or parsed.then_action.lower() in action.id.lower():
                bonus += 0.15
    return min(bonus, 0.45)


def score_action(action: CandidateAction, context: dict[str, Any]) -> ScoredAction:
    """3D score: vision × correctness × safety, plus IF bonuses."""
    safety = 1.0 - max(0.0, min(1.0, action.risk_score))
    if_extra = if_bonus_for_action(action, context)
    total = (
        0.35 * action.vision_score
        + 0.40 * action.correctness_score
        + 0.25 * safety
        + if_extra
    )
    rationale = (
        f"vision={action.vision_score:.2f} correctness={action.correctness_score:.2f} "
        f"safety={safety:.2f} if_bonus={if_extra:.2f}"
    )
    return ScoredAction(
        action_id=action.id,
        label=action.label,
        total=round(total, 4),
        vision=action.vision_score,
        correctness=action.correctness_score,
        safety=safety,
        if_bonus=if_extra,
        rationale=rationale,
    )


class DecisionEngine:
    """Autonomous picker — always chooses the highest-scoring legal move."""

    def __init__(self, context: dict[str, Any] | None = None) -> None:
        self.context = context or {}

    def best_move(self, candidates: list[CandidateAction]) -> ScoredAction:
        if not candidates:
            raise ValueError("no candidate actions to evaluate")
        scored = [score_action(c, self.context) for c in candidates]
        return max(scored, key=lambda s: s.total)

    def rank(self, candidates: list[CandidateAction]) -> list[ScoredAction]:
        scored = [score_action(c, self.context) for c in candidates]
        return sorted(scored, key=lambda s: s.total, reverse=True)


def evaluate_candidates(payload: dict[str, Any]) -> dict[str, Any]:
    context = payload.get("context") or {}
    raw_candidates = payload.get("candidates") or []
    candidates = [
        CandidateAction(
            id=str(c.get("id", f"action-{i}")),
            label=str(c.get("label", c.get("id", f"action-{i}"))),
            description=str(c.get("description", "")),
            vision_score=float(c.get("vision_score", 0.5)),
            correctness_score=float(c.get("correctness_score", 0.5)),
            risk_score=float(c.get("risk_score", 0.5)),
            tags=list(c.get("tags") or []),
            if_rules=list(c.get("if_rules") or []),
        )
        for i, c in enumerate(raw_candidates)
    ]
    engine = DecisionEngine(context=context)
    if not candidates:
        raise ValueError("no candidate actions to evaluate")
    ranked = engine.rank(candidates)
    best = ranked[0]
    return {
        "best": {
            "action_id": best.action_id,
            "label": best.label,
            "total": best.total,
            "rationale": best.rationale,
        },
        "ranked": [
            {
                "action_id": s.action_id,
                "label": s.label,
                "total": s.total,
                "vision": s.vision,
                "correctness": s.correctness,
                "safety": s.safety,
                "if_bonus": s.if_bonus,
                "rationale": s.rationale,
            }
            for s in ranked
        ],
    }


def main_stdin() -> dict[str, Any]:
    import sys

    raw = sys.stdin.read()
    payload = json.loads(raw) if raw.strip() else {}
    mode = payload.get("mode", "decide")
    if mode == "decide":
        return evaluate_candidates(payload)
    raise ValueError(f"unknown mode: {mode}")
