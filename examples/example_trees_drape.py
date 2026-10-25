"""Tree heights on the same 2 km × 2 km crop as ``example_terrain.py``.

500 trees on a 25×20 grid.

Default: rust ``drape_vertices`` on 4 synthetic samples (always runnable).
``--bimfabrik``: intern trees path — sample the four DGM1 GeoTIFFs in Python
(``extract_elevation_df_from_geotiff``), then rust only writes (no mesh drape).
"""

from __future__ import annotations

import argparse
import math
import re
import time
from pathlib import Path

import numpy as np
import pandas as pd
import rasterio
from scipy.spatial import cKDTree

from bimfabrikhh_core_rs import trees_to_ifc
from bimfabrikhh_core_rs.crs import etrs89_utm
from bimfabrikhh_core_rs.owner import defaults
from bimfabrikhh_core_rs.tree_mapping import specs

from envpaths import ROOT, dgm_dir
from logconfig import ENABLED, ifc_created, log

OUTPUT = ROOT / "output" / "example_trees_drape.ifc"
OUTPUT_2KM = ROOT / "output" / "example_trees_drape_2km.ifc"

_TILES = (
    "dgm1_32_565_9330_1_hh_2022.tif",
    "dgm1_32_565_9340_1_hh_2022.tif",
    "dgm1_32_566_9330_1_hh_2022.tif",
    "dgm1_32_566_9340_1_hh_2022.tif",
)
_BBOX = (565000.0, 5933000.0, 567000.0, 5935000.0)
_N_TREES = 500
_GRID_COLS = 25
_MAX_ABS_Z = 10_000.0

_SYNTHETIC_DRAPE = [
    [565000.0, 5933000.0, 4.0],
    [567000.0, 5933000.0, 6.5],
    [565000.0, 5935000.0, 12.0],
    [566000.0, 5934000.0, 8.0],
]


def _tree(name: str, x: float, y: float, z: float) -> dict:
    return {
        "name": name,
        "position": (x, y, z),
        "trunk_radius": 0.3,
        "trunk_height": 4.0,
        "crown_radius": 2.5,
        "detail": 1,
        "segments": 8,
        "attributes": {"baumnummer": name, "gattung_deutsch": "Eiche", "art_baum": "Quercus robur"},
    }


def _trees_in_bbox(*, keep_one_z: bool) -> list[dict]:
    min_x, min_y, max_x, max_y = _BBOX
    rows = math.ceil(_N_TREES / _GRID_COLS)
    dx = (max_x - min_x) / max(_GRID_COLS - 1, 1)
    dy = (max_y - min_y) / max(rows - 1, 1)
    keep = _N_TREES // 2
    trees = []
    for i in range(_N_TREES):
        col, row = i % _GRID_COLS, i // _GRID_COLS
        z = 5.0 if keep_one_z and i == keep else 0.0
        name = "Baum_keep_z" if keep_one_z and i == keep else f"Baum_{i + 1:03d}"
        trees.append(_tree(name, min_x + col * dx, min_y + row * dy, z))
    return trees


def _elevation_ok(z: float) -> bool:
    return math.isfinite(z) and abs(z) < _MAX_ABS_Z


def _placement_z(ifc_text: str) -> dict[str, float]:
    points = {
        m.group(1): tuple(float(v) for v in m.group(2).split(","))
        for m in re.finditer(r"#(\d+)=IFCCARTESIANPOINT\(\(([^)]+)\)\);", ifc_text)
    }
    axes = {m.group(1): m.group(2) for m in re.finditer(r"#(\d+)=IFCAXIS2PLACEMENT3D\(#(\d+),", ifc_text)}
    places = {
        m.group(1): m.group(2) for m in re.finditer(r"#(\d+)=IFCLOCALPLACEMENT\(#\d+,#(\d+)\);", ifc_text)
    }
    out: dict[str, float] = {}
    for m in re.finditer(
        r"#\d+=IFCBUILDINGELEMENTPROXY\('[^']+',#\d+,'([^']+)',\$,\$,#(\d+),",
        ifc_text,
    ):
        name, place = m.group(1), m.group(2)
        axis = places.get(place)
        pt = axes.get(axis) if axis else None
        xyz = points.get(pt) if pt else None
        if xyz is not None and len(xyz) == 3:
            out[name] = xyz[2]
    return out


def _check_placements(trees: list[dict], path: Path) -> None:
    got = _placement_z(path.read_text())
    z_min, z_max = math.inf, -math.inf
    for tree in trees:
        name = tree["name"]
        z_exp = tree["position"][2]
        z_got = got.get(name)
        if z_got is None:
            raise SystemExit(f"missing placement for {name}")
        if abs(z_got - z_exp) > 1e-4:
            raise SystemExit(f"{name}: expected z={z_exp}, IFC has z={z_got}")
        z_min = min(z_min, z_got)
        z_max = max(z_max, z_got)
    log.info("ok %d/%d placements  z=%.2f…%.2f", len(trees), len(trees), z_min, z_max)


def _write(trees: list[dict], path: Path, *, drape_vertices=None, drape_faces=None) -> str:
    path.parent.mkdir(parents=True, exist_ok=True)
    t0 = time.perf_counter()
    written = trees_to_ifc(
        trees,
        str(path),
        crs=etrs89_utm(32, vertical_datum="EPSG:7837"),
        psets=specs(),
        owner=defaults(),
        basepoint=(_BBOX[0], _BBOX[1]),
        drape_vertices=drape_vertices,
        drape_faces=drape_faces,
        progress=ENABLED,
    )
    ifc_created(written, time.perf_counter() - t0)
    _check_placements(trees if drape_vertices is None else _trees_after_expected_drape(trees, drape_vertices), Path(written))
    return written


def _trees_after_expected_drape(trees: list[dict], samples: list[list[float]]) -> list[dict]:
    """Copy trees with rust nearest-vertex Z for the synthetic check."""
    pts = np.asarray(samples, dtype=float)
    ok = np.isfinite(pts[:, 2]) & (np.abs(pts[:, 2]) < _MAX_ABS_Z)
    pts = pts[ok]
    tree_idx = cKDTree(pts[:, :2])
    out = []
    for t in trees:
        x, y, z = t["position"]
        if z == 0.0 or not _elevation_ok(z):
            _, i = tree_idx.query([x, y])
            z = float(pts[int(i), 2])
        nt = dict(t)
        nt["position"] = (x, y, z)
        out.append(nt)
    return out


def _sample_tif_elevations(trees: list[dict]) -> None:
    """Intern trees path: GeoTIFF sample per XY, then nearest-valid fill."""
    from BIMFabrikHH_core.core.georeferencing.extract_elevation import (
        extract_elevation_df_from_geotiff,
        fill_nodata_from_nearest,
    )

    tifs = [dgm_dir() / name for name in _TILES]
    missing = [p for p in tifs if not p.is_file()]
    if missing:
        raise FileNotFoundError("DGM tiles not found:\n" + "\n".join(str(p) for p in missing))

    xs = np.array([t["position"][0] for t in trees], dtype=float)
    ys = np.array([t["position"][1] for t in trees], dtype=float)
    zs = np.full(len(trees), np.nan)

    t0 = time.perf_counter()
    for path in tifs:
        with rasterio.open(path) as src:
            b = src.bounds
        mask = (xs >= b.left) & (xs <= b.right) & (ys >= b.bottom) & (ys <= b.top)
        if not np.any(mask):
            continue
        df = pd.DataFrame({"Easting": xs[mask], "Northing": ys[mask]})
        df = extract_elevation_df_from_geotiff(df, str(path), "Easting", "Northing", "Elevation")
        zs[mask] = df["Elevation"].to_numpy()
    zs, filled = fill_nodata_from_nearest(xs, ys, zs)
    log.info(
        "tif sample: %.3fs  %d trees  %d tiles  filled_nodata=%d  z=%.2f…%.2f",
        time.perf_counter() - t0,
        len(trees),
        len(tifs),
        filled,
        float(np.min(zs)),
        float(np.max(zs)),
    )
    for tree, z in zip(trees, zs):
        x, y, _ = tree["position"]
        tree["position"] = (x, y, float(z))


def main(*, bimfabrik: bool = False) -> None:
    if bimfabrik:
        trees = _trees_in_bbox(keep_one_z=False)
        log.info("intern trees: GeoTIFF sample  %d trees  bbox=%s", len(trees), _BBOX)
        _sample_tif_elevations(trees)
        _write(trees, OUTPUT_2KM)
        return
    trees = _trees_in_bbox(keep_one_z=True)
    log.info("synthetic rust drape: %d trees in 2 km bbox %s", len(trees), _BBOX)
    _write(trees, OUTPUT, drape_vertices=_SYNTHETIC_DRAPE)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Drape trees on the example_terrain 2 km crop")
    parser.add_argument(
        "--bimfabrik",
        action="store_true",
        help="sample DGM1 GeoTIFFs like intern trees, then rust write only",
    )
    main(bimfabrik=parser.parse_args().bimfabrik)
