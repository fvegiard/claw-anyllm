"""Autonomous evaluation: 3D vision scoring + IF-aware best-move selection."""

from .decision_engine import DecisionEngine, evaluate_candidates

__all__ = ["DecisionEngine", "evaluate_candidates", "score_vision_3d"]


def score_vision_3d(image_path: str):
    from .vision_eval import score_vision_3d as _score

    return _score(image_path)
