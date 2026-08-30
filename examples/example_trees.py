"""Synthetic trees → IFC via Rust (no ifcfactory).

Writes ``example_trees.ifc`` (30 trees, crown detail 1).
Also write the 1200-tree file with ``LARGE = True`` or ``python example_trees.py --large``.
"""

from __future__ import annotations

import argparse
import random
import time
from pathlib import Path

from bimfabrikhh_core_rs import trees_to_ifc
from bimfabrikhh_core_rs.owner import defaults
from bimfabrikhh_core_rs.tree_mapping import specs

from envpaths import ROOT
from logconfig import ENABLED, ifc_created, log

# Set True to also write the 1200-tree file.
LARGE = False

OUTPUT_SMALL = ROOT / "output" / "example_trees.ifc"
OUTPUT_LARGE = ROOT / "output" / "example_trees_large.ifc"

ORIGIN = (565100.0, 5933600.0, 0.0)
SPACING_M = 8.0
SEED = 1

# Crown is the size driver. Trunk radius and height follow it so a tall tree
# never gets a small crown (same fallback as BIMFabrik ``resolve_tree_dimensions``).
_MIN_TRUNK_HEIGHT_M = 3.5
_TRUNK_HEIGHT_CROWN_FACTOR = 1.35
_SMALL_CROWN_DIAMETER_M = 3.0
_TRUNK_TO_CROWN = 0.08
_CROWN_R_MIN = 1.2
_CROWN_R_MAX = 6.4
_GATTUNG = (
    ("Eiche", "Quercus robur"),
    ("Buche", "Fagus sylvatica"),
    ("Linde", "Tilia cordata"),
    ("Ahorn", "Acer platanoides"),
)


def _trunk_height(crown_radius: float) -> float:
    crown_diameter = crown_radius * 2
    if crown_diameter < _SMALL_CROWN_DIAMETER_M:
        return _MIN_TRUNK_HEIGHT_M
    return _TRUNK_HEIGHT_CROWN_FACTOR * crown_diameter


def _dims(rng: random.Random) -> tuple[float, float, float]:
    crown_r = rng.uniform(_CROWN_R_MIN, _CROWN_R_MAX)
    trunk_r = crown_r * _TRUNK_TO_CROWN
    return trunk_r, _trunk_height(crown_r), crown_r


def _tree(i: int, *, cols: int, detail: int, name_width: int, rng: random.Random) -> dict:
    col, row = i % cols, i // cols
    trunk_r, trunk_h, crown_r = _dims(rng)
    gattung, art = _GATTUNG[i % len(_GATTUNG)]
    name = f"Baum_{i + 1:0{name_width}d}"
    return {
        "name": name,
        "position": (ORIGIN[0] + col * SPACING_M, ORIGIN[1] + row * SPACING_M, ORIGIN[2]),
        "trunk_radius": trunk_r,
        "trunk_height": trunk_h,
        "crown_radius": crown_r,
        "detail": detail,
        "segments": 8,
        "attributes": {
            "baumnummer": name.split("_")[-1],
            "gattung_deutsch": gattung,
            "art_baum": art,
            "pflanzjahr": rng.randint(1970, 2018),
            "bezirk": "Hamburg-Mitte",
            "strassenname": "Beispielstraße",
        },
    }


def _write(path: Path, n: int, *, cols: int, detail: int) -> str:
    path.parent.mkdir(parents=True, exist_ok=True)
    rows = (n + cols - 1) // cols
    name_width = max(3, len(str(n)))
    log.debug("output:  %s", path)
    log.debug("trees:   %d  grid=%dx%d  spacing=%.1f m", n, cols, rows, SPACING_M)
    log.debug("mesh:    crown detail=%d  trunk segments=8", detail)
    log.debug("origin:  %s  epsg=25832  seed=%d", ORIGIN, SEED)

    t0 = time.perf_counter()
    rng = random.Random(SEED)
    trees = [_tree(i, cols=cols, detail=detail, name_width=name_width, rng=rng) for i in range(n)]
    log.debug("prepare: %d tree dict(s) in %.3fs", len(trees), time.perf_counter() - t0)
    log.debug(
        "dims:    trunk r %.2f–%.2f m  height %.1f–%.1f m  crown r %.1f–%.1f m",
        min(t["trunk_radius"] for t in trees),
        max(t["trunk_radius"] for t in trees),
        min(t["trunk_height"] for t in trees),
        max(t["trunk_height"] for t in trees),
        min(t["crown_radius"] for t in trees),
        max(t["crown_radius"] for t in trees),
    )

    t1 = time.perf_counter()
    written = trees_to_ifc(
        trees,
        str(path),
        epsg=25832,
        psets=specs(),
        owner=defaults(),
        basepoint=(ORIGIN[0], ORIGIN[1]),
        progress=ENABLED,
    )
    elapsed = time.perf_counter() - t1
    log.debug("trees_to_ifc: %.2fs", elapsed)
    ifc_created(written, elapsed)
    return written


def main(large: bool = False) -> None:
    log.info("small: 30 trees, crown detail 1")
    _write(OUTPUT_SMALL, 30, cols=6, detail=4)
    if large:
        log.info("large: 1200 trees, crown detail 4")
        _write(OUTPUT_LARGE, 1200, cols=40, detail=4)
    else:
        log.info("skip large (pass --large or set LARGE = True)")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Synthetic trees → IFC via Rust")
    parser.add_argument(
        "--large",
        action="store_true",
        help="also write 1200 trees at crown detail 4 (off by default)",
    )
    main(large=parser.parse_args().large or LARGE)
