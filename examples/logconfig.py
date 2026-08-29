"""Example logging.

``INFO`` (IFC created) is on by default.
``DEBUG`` and Rust parse/write progress stay off unless ``ENABLED`` is True.
"""

from __future__ import annotations

import logging
from pathlib import Path

# Set True for DEBUG details and Rust parse/write progress on stderr.
ENABLED = True

log = logging.getLogger("bimfabrikhh_core_rs.examples")
log.propagate = False
log.handlers.clear()
log.disabled = False
handler = logging.StreamHandler()
handler.setFormatter(logging.Formatter("%(levelname)s %(message)s"))
log.addHandler(handler)
log.setLevel(logging.DEBUG if ENABLED else logging.INFO)


def ifc_created(path: str | Path, seconds: float) -> None:
    size = Path(path).stat().st_size / 1e6
    log.info("IFC created: %s (%.2f MB, %.2fs)", path, size, seconds)
