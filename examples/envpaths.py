"""Load example data folders from the repo-root ``.env``. Paths stay out of git."""

from __future__ import annotations

import os
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
_LOADED = False

_LOD1 = "BIMFABRIKHH_LOD1_GML_DIR"
_LOD2 = "BIMFABRIKHH_LOD2_GML_DIR"
_LOD3 = "BIMFABRIKHH_LOD3_GML_DIR"
_DGM = "BIMFABRIKHH_DGM_TIF_DIR"
FIXTURES = ROOT / "fixtures"


def _load_dotenv() -> None:
    global _LOADED
    if _LOADED:
        return
    _LOADED = True
    path = ROOT / ".env"
    if not path.is_file():
        return
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, _, value = line.partition("=")
        key = key.strip()
        value = value.strip().strip("'").strip('"')
        if key and key not in os.environ:
            os.environ[key] = value


def _env_path(name: str) -> Path | None:
    _load_dotenv()
    raw = os.environ.get(name, "").strip()
    if not raw:
        return None
    return Path(raw).expanduser()


def _require_dir(name: str) -> Path:
    path = _env_path(name)
    if path is None:
        raise FileNotFoundError(f"{name} is not set. Copy .env.example to .env and set the local folder.")
    if not path.is_dir():
        raise FileNotFoundError(f"{name} is not a directory: {path}")
    return path


def lod1_dir() -> Path:
    return _require_dir(_LOD1)


def lod2_dir() -> Path:
    return _require_dir(_LOD2)


def lod3_dir() -> Path:
    return _require_dir(_LOD3)


def dgm_dir() -> Path:
    return _require_dir(_DGM)
