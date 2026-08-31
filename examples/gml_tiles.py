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
    r"<(?:gml:)?(lower|upper)Corner(?:\s[^>]*)?>\s*([+-]?\d+(?:\.\d+)?)\s+([+-]?\d+(?:\.\d+)?)"
)
_LOD12_NAME = re.compile(r"^LoD[12]_32_(\d+)_(\d+)_", re.IGNORECASE)
_TILE_SUFFIXES = {".gml", ".xml"}
Envelope = tuple[float, float, float, float]
TileIndex = list[tuple[Path, Envelope | None]]


def tile_files(folder: Path) -> list[Path]:
    """CityGML tiles in a folder (``.gml`` or ``.xml``). Zips and rasters are skipped."""
    return sorted(
        p for p in folder.iterdir() if p.is_file() and p.suffix.lower() in _TILE_SUFFIXES
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
                found = tile_files(tiles)
                return found[:1]
            path = tiles / tile
            if not path.is_file():
                raise FileNotFoundError(path)
            return [path]
        return tile_files(tiles)
    paths = sorted(tiles)
    return paths if loop else paths[:1]


def _envelope_from_name(path: Path) -> Envelope | None:
    """Hamburg LoD1/LoD2: ``LoD1_32_565_5933_1_HH`` is the 1 km cell at (565000, 5933000)."""
    match = _LOD12_NAME.match(path.stem)
    if match is None:
        return None
    east_km, north_km = int(match.group(1)), int(match.group(2))
    x0, y0 = east_km * 1000.0, north_km * 1000.0
    return x0, y0, x0 + 1000.0, y0 + 1000.0


def _envelope(path: Path) -> Envelope | None:
    named = _envelope_from_name(path)
    if named is not None:
        return named
    with path.open(encoding="utf-8", errors="replace") as fh:
        head = fh.read(4000)
    found: dict[str, tuple[float, float]] = {}
    for kind, x, y in _CORNER.findall(head):
        found[kind] = (float(x), float(y))
    if "lower" not in found or "upper" not in found:
        return None
    return found["lower"][0], found["lower"][1], found["upper"][0], found["upper"][1]


def tile_envelopes(folder: Path) -> TileIndex:
    """Read each tile envelope once (filename grid for Hamburg LoD1/LoD2)."""
    return [(path, _envelope(path)) for path in tile_files(folder)]


def folder_extent(folder: Path, envelopes: TileIndex | None = None) -> Envelope:
    """Union of tile envelopes, or raise if none can be read."""
    items = envelopes if envelopes is not None else tile_envelopes(folder)
    boxes = [env for _, env in items if env is not None]
    if not boxes:
        raise FileNotFoundError(f"no gml:Envelope in {folder}")
    return (
        min(b[0] for b in boxes),
        min(b[1] for b in boxes),
        max(b[2] for b in boxes),
        max(b[3] for b in boxes),
    )


def tiles_in_bbox(
    folder: Path,
    bbox: Envelope,
    envelopes: TileIndex | None = None,
) -> list[Path]:
    minx, miny, maxx, maxy = bbox
    items = envelopes if envelopes is not None else tile_envelopes(folder)
    hits: list[Path] = []
    for path, env in items:
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
    envelopes: TileIndex | None = None,
    allow_empty: bool = False,
) -> Path | None:
    paths = tiles_in_bbox(tiles, bbox_epsg, envelopes) if tiles.is_dir() else [tiles]
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
    try:
        written = gml_to_ifc([str(p) for p in paths], str(ifc_path), **kwargs)
    except ValueError as exc:
        if allow_empty and "no buildings parsed" in str(exc):
            log.info("skip empty crop (tiles hit the umring, no buildings inside)")
            return None
        raise
    elapsed = time.perf_counter() - t0
    log.debug(f"gml_to_ifc: {elapsed:.2f}s")
    ifc_created(written, elapsed)
    return Path(written)


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
