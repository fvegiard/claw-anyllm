"""Tests for autonomous IF + 3D decision engine."""

from __future__ import annotations

import unittest

from claw_eval.decision_engine import CandidateAction, DecisionEngine, parse_if_rule


class DecisionEngineTests(unittest.TestCase):
    def test_parse_if_rule(self) -> None:
        rule = parse_if_rule("if ui_changed == true then vision-verify")
        self.assertIsNotNone(rule)
        assert rule is not None
        self.assertEqual(rule.field, "ui_changed")
        self.assertEqual(rule.operator, "==")
        self.assertTrue(rule.value)

    def test_best_move_prefers_vision_when_ui_changed(self) -> None:
        engine = DecisionEngine(context={"ui_changed": True})
        ranked = engine.rank(
            [
                CandidateAction(
                    id="vision",
                    label="vision-verify",
                    vision_score=0.95,
                    correctness_score=0.7,
                    risk_score=0.2,
                    if_rules=["if ui_changed == true then vision-verify"],
                ),
                CandidateAction(
                    id="ship",
                    label="ship-fast",
                    vision_score=0.2,
                    correctness_score=0.8,
                    risk_score=0.3,
                ),
            ]
        )
        self.assertEqual(ranked[0].action_id, "vision")
        self.assertGreater(ranked[0].if_bonus, 0.0)


if __name__ == "__main__":
    unittest.main()
