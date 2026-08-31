"""Load example data folders from the repo-root ``.env``. Paths stay out of git."""

from __future__ import annotations

import os
import re
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


_MNT_DRIVE = re.compile(r"^/mnt/([a-zA-Z])(?:/(.*))?$")
_WIN_DRIVE = re.compile(r"^([a-zA-Z]):(?:[\\/](.*))?$")


def _as_windows(raw: str) -> Path | None:
    posix = raw.replace("\\", "/")
    mnt = _MNT_DRIVE.match(posix)
    if mnt:
        rest = (mnt.group(2) or "").replace("/", "\\")
        return Path(f"{mnt.group(1).upper()}:\\" + rest)
    win = _WIN_DRIVE.match(posix)
    if win:
        rest = (win.group(2) or "").replace("/", "\\")
        return Path(f"{win.group(1).upper()}:\\" + rest)
    return None


def _as_mnt(raw: str) -> Path | None:
    posix = raw.replace("\\", "/")
    mnt = _MNT_DRIVE.match(posix)
    if mnt:
        return Path(posix)
    win = _WIN_DRIVE.match(posix)
    if win:
        rest = win.group(2) or ""
        return Path(f"/mnt/{win.group(1).lower()}/{rest}")
    return None


def _resolve_dir(raw: str) -> Path | None:
    """Windows drive first (``C:\\…``), then WSL ``/mnt/c/…``."""
    seen: list[Path] = []
    for candidate in (_as_windows(raw), _as_mnt(raw), Path(raw).expanduser()):
        if candidate is None or candidate in seen:
            continue
        seen.append(candidate)
        if candidate.is_dir():
            return candidate
    return None


def _env_path(name: str) -> Path | None:
    _load_dotenv()
    raw = os.environ.get(name, "").strip()
    if not raw:
        return None
    return _resolve_dir(raw) or Path(raw).expanduser()


def _require_dir(name: str) -> Path:
    _load_dotenv()
    raw = os.environ.get(name, "").strip()
    if not raw:
        raise FileNotFoundError(f"{name} is not set. Copy .env.example to .env and set the local folder.")
    path = _resolve_dir(raw)
    if path is None:
        tried = [p for p in (_as_windows(raw), _as_mnt(raw), Path(raw).expanduser()) if p is not None]
        raise FileNotFoundError(f"{name} is not a directory (tried: {', '.join(str(p) for p in tried)})")
    return path


def lod1_dir() -> Path:
    return _require_dir(_LOD1)


def lod2_dir() -> Path:
    return _require_dir(_LOD2)


def lod3_dir() -> Path:
    return _require_dir(_LOD3)


def dgm_dir() -> Path:
    return _require_dir(_DGM)
