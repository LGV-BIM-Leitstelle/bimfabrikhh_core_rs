"""Shared CityGML tile → IFC loop for the examples."""

from __future__ import annotations

import re
import time
from collections.abc import Iterable, Mapping
from pathlib import Path

from bimfabrikhh_core_rs import gml_to_ifc
from bimfabrikhh_core_rs.attribute_mapping import specs

from logconfig import ENABLED, ifc_created, log

_CORNER = re.compile(
    r"<(?:gml:)?(lower|upper)Corner>\s*([+-]?\d+(?:\.\d+)?)\s+([+-]?\d+(?:\.\d+)?)"
)


def _tile_paths(
    tiles: Path | Iterable[Path],
    loop: bool,
    tile: str | None,
) -> list[Path]:
    if isinstance(tiles, Path):
        if tiles.is_file():
            return [tiles]
        if not tiles.is_dir():
            raise FileNotFoundError(tiles)
        if not loop:
            if tile is None:
                found = sorted(tiles.glob("*.gml"))
                return found[:1]
            path = tiles / tile
            if not path.is_file():
                raise FileNotFoundError(path)
            return [path]
        return sorted(tiles.glob("*.gml"))
    paths = sorted(tiles)
    return paths if loop else paths[:1]


def _envelope(path: Path) -> tuple[float, float, float, float] | None:
    with path.open(encoding="utf-8", errors="replace") as fh:
        head = fh.read(4000)
    found: dict[str, tuple[float, float]] = {}
    for kind, x, y in _CORNER.findall(head):
        found[kind] = (float(x), float(y))
    if "lower" not in found or "upper" not in found:
        return None
    return found["lower"][0], found["lower"][1], found["upper"][0], found["upper"][1]


def tiles_in_bbox(folder: Path, bbox: tuple[float, float, float, float]) -> list[Path]:
    minx, miny, maxx, maxy = bbox
    hits: list[Path] = []
    for path in sorted(folder.glob("*.gml")):
        env = _envelope(path)
        if env is None:
            hits.append(path)
            continue
        ex0, ey0, ex1, ey1 = env
        if ex1 >= minx and ey1 >= miny and ex0 <= maxx and ey0 <= maxy:
            hits.append(path)
    return hits


def convert_gml_crop(
    tiles: Path,
    *,
    ifc_path: Path,
    mode: str,
    epsg: int,
    bbox_epsg: tuple[float, float, float, float],
    ifc_types: Mapping[str, str] | None = None,
    site_name: str = "Hamburg_Site",
    project_name: str = "Hamburg_LoD3",
) -> None:
    paths = tiles_in_bbox(tiles, bbox_epsg) if tiles.is_dir() else [tiles]
    if not paths:
        raise FileNotFoundError(f"no *.gml in {tiles} intersects bbox {bbox_epsg}")
    ifc_path.parent.mkdir(parents=True, exist_ok=True)
    log.debug(f"mode: {mode}")
    log.debug(f"crop: EPSG:{epsg} {bbox_epsg}")
    log.debug(f"tiles hitting bbox: {len(paths)}")
    t0 = time.perf_counter()
    kwargs: dict = {
        "mode": mode,
        "project_name": project_name,
        "site_name": site_name,
        "epsg": epsg,
        "bbox_epsg": bbox_epsg,
        "basepoint": (bbox_epsg[0], bbox_epsg[1]),
        "psets": specs(),
        "progress": ENABLED,
    }
    if ifc_types is not None:
        kwargs["ifc_types"] = ifc_types
    written = gml_to_ifc([str(p) for p in paths], str(ifc_path), **kwargs)
    elapsed = time.perf_counter() - t0
    log.debug(f"gml_to_ifc: {elapsed:.2f}s")
    ifc_created(written, elapsed)


def convert_gml_tiles(
    tiles: Path | Iterable[Path],
    *,
    output: Path,
    prefix: str,
    mode: str,
    epsg: int,
    ifc_types: Mapping[str, str] | None = None,
    site_name: str = "Site",
    loop: bool = False,
    tile: str | None = None,
) -> None:
    paths = _tile_paths(tiles, loop, tile)
    if not paths:
        loc = tiles if isinstance(tiles, Path) else "tile list"
        raise FileNotFoundError(f"no *.gml in {loc}")
    output.mkdir(parents=True, exist_ok=True)

    log.debug(f"mode: {mode}")
    log.debug(f"tiles: {len(paths)}")

    for gml in paths:
        ifc_path = output / f"{prefix}_{gml.stem}.ifc"
        log.debug(f"\nGML: {gml.name} ({gml.stat().st_size / 1e6:.1f} MB)")
        t0 = time.perf_counter()
        kwargs: dict = {
            "mode": mode,
            "project_name": gml.stem,
            "site_name": site_name,
            "epsg": epsg,
            "psets": specs(),
            "progress": ENABLED,
        }
        if ifc_types is not None:
            kwargs["ifc_types"] = ifc_types
        written = gml_to_ifc(str(gml), str(ifc_path), **kwargs)
        elapsed = time.perf_counter() - t0
        log.debug(f"gml_to_ifc: {elapsed:.2f}s")
        ifc_created(written, elapsed)
