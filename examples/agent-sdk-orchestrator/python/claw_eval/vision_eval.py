"""3-dimensional vision scoring for UI/screenshot evaluation."""

from __future__ import annotations

import math
from pathlib import Path
from typing import Any

import numpy as np
from PIL import Image


def _load_gray(path: Path) -> np.ndarray:
    with Image.open(path) as img:
        rgb = img.convert("RGB")
        arr = np.asarray(rgb, dtype=np.float32) / 255.0
    # luminance
    return 0.2126 * arr[:, :, 0] + 0.7152 * arr[:, :, 1] + 0.0722 * arr[:, :, 2]


def _spatial_balance(gray: np.ndarray) -> float:
    """Axis 1: layout balance (left/right, top/bottom mass)."""
    h, w = gray.shape
    left = gray[:, : w // 2].mean()
    right = gray[:, w // 2 :].mean()
    top = gray[: h // 2, :].mean()
    bottom = gray[h // 2 :, :].mean()
    lr = 1.0 - min(1.0, abs(left - right) * 2.0)
    tb = 1.0 - min(1.0, abs(top - bottom) * 2.0)
    return float((lr + tb) / 2.0)


def _depth_proxy(gray: np.ndarray) -> float:
    """Axis 2: depth proxy via edge density + vertical gradient (no true 3D without depth cam)."""
    gy, gx = np.gradient(gray)
    edge = np.hypot(gx, gy)
    edge_density = float(np.clip(edge.mean() * 4.0, 0.0, 1.0))
    vertical_grad = float(np.clip(abs(gy).mean() * 3.0, 0.0, 1.0))
    return float(0.6 * edge_density + 0.4 * vertical_grad)


def _clarity(gray: np.ndarray) -> float:
    """Axis 3: clarity / contrast (readable UI)."""
    std = float(gray.std())
    return float(np.clip(std * 3.5, 0.0, 1.0))


def score_vision_3d(image_path: str | Path) -> dict[str, Any]:
    path = Path(image_path)
    if not path.is_file():
        return {"error": f"image not found: {path}", "composite": 0.0}

    gray = _load_gray(path)
    spatial = _spatial_balance(gray)
    depth = _depth_proxy(gray)
    clarity = _clarity(gray)
    composite = 0.34 * spatial + 0.33 * depth + 0.33 * clarity

    return {
        "path": str(path),
        "dimensions": {
            "spatial_balance": round(spatial, 4),
            "depth_proxy": round(depth, 4),
            "clarity": round(clarity, 4),
        },
        "composite": round(composite, 4),
        "library": "Pillow+numpy",
    }


def score_vision_opencv(image_path: str | Path) -> dict[str, Any] | None:
    try:
        import cv2  # type: ignore
    except ImportError:
        return None

    path = Path(image_path)
    bgr = cv2.imread(str(path))
    if bgr is None:
        return {"error": f"opencv could not read: {path}", "composite": 0.0}

    gray = cv2.cvtColor(bgr, cv2.COLOR_BGR2GRAY)
    lap_var = float(cv2.Laplacian(gray, cv2.CV_64F).var())
    clarity = float(np.clip(lap_var / 500.0, 0.0, 1.0))
    edges = cv2.Canny(gray, 50, 150)
    edge_ratio = float(edges.mean())
    depth = float(np.clip(edge_ratio * 2.5, 0.0, 1.0))
    h, w = gray.shape
    spatial = 1.0 - min(
        1.0,
        abs(gray[:, : w // 2].mean() - gray[:, w // 2 :].mean()) * 2.0,
    )
    composite = 0.34 * spatial + 0.33 * depth + 0.33 * clarity
    return {
        "path": str(path),
        "dimensions": {
            "spatial_balance": round(spatial, 4),
            "depth_proxy": round(depth, 4),
            "clarity": round(clarity, 4),
        },
        "composite": round(composite, 4),
        "library": "opencv-python-headless",
    }


def best_vision_score(image_path: str | Path) -> dict[str, Any]:
    """Pick best available Python vision backend."""
    opencv = score_vision_opencv(image_path)
    if opencv is not None and "error" not in opencv:
        return opencv
    return score_vision_3d(image_path)
