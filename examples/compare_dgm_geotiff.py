"""1000×1000 grid: IFC DGM mesh Z vs DGM1 GeoTIFF at the same XY.

Uses ``output/example_terrain_2km.ifc`` and the four tiles in
``BIMFABRIKHH_DGM_TIF_DIR``. Writes ``output/dgm_vs_geotiff_1000.npz``.
"""

from __future__ import annotations

import re
import time
from pathlib import Path

import numpy as np
import rasterio
from scipy.interpolate import LinearNDInterpolator

from envpaths import ROOT, dgm_dir
from logconfig import log

IFC = ROOT / "output" / "example_terrain_2km.ifc"
OUTPUT = ROOT / "output" / "dgm_vs_geotiff_1000.npz"
_TILES = (
    "dgm1_32_565_9330_1_hh_2022.tif",
    "dgm1_32_565_9340_1_hh_2022.tif",
    "dgm1_32_566_9330_1_hh_2022.tif",
    "dgm1_32_566_9340_1_hh_2022.tif",
)
_BBOX = (565000.0, 5933000.0, 567000.0, 5935000.0)
_N = 1000
_MAX_ABS_Z = 10_000.0


def _elevation_ok(z: np.ndarray) -> np.ndarray:
    return np.isfinite(z) & (np.abs(z) < _MAX_ABS_Z)


def _parse_terrain_ifc(path: Path) -> tuple[np.ndarray, np.ndarray]:
    text = path.read_text()
    points = {
        m.group(1): tuple(float(v) for v in m.group(2).split(","))
        for m in re.finditer(r"#(\d+)=IFCCARTESIANPOINT\(\(([^)]+)\)\);", text)
    }
    axes = {m.group(1): m.group(2) for m in re.finditer(r"#(\d+)=IFCAXIS2PLACEMENT3D\(#(\d+),", text)}
    places = {
        m.group(1): m.group(2) for m in re.finditer(r"#(\d+)=IFCLOCALPLACEMENT\(#\d+,#(\d+)\);", text)
    }
    origin = (565000.0, 5933000.0, 0.0)
    for m in re.finditer(
        r"#\d+=IFCBUILDINGELEMENTPROXY\('[^']+',#\d+,'([^']+)',\$,\$,#(\d+),",
        text,
    ):
        if m.group(1) != "DGM":
            continue
        axis = places.get(m.group(2))
        pt = axes.get(axis) if axis else None
        xyz = points.get(pt) if pt else None
        if xyz is not None and len(xyz) == 3:
            origin = xyz
            break

    m = re.search(r"IFCCARTESIANPOINTLIST3D\(\(\((.+)\)\)\);", text)
    if not m:
        raise SystemExit(f"no IfcCartesianPointList3D in {path}")
    verts = np.array(
        [[float(v) for v in p.strip("()").split(",")] for p in m.group(1).split("),(")],
        dtype=np.float64,
    )
    verts[:, 0] += origin[0]
    verts[:, 1] += origin[1]
    verts[:, 2] += origin[2]

    faces = np.array(
        [
            [int(v) - 1 for v in m.group(1).split(",")[:3]]
            for m in re.finditer(r"IFCINDEXEDPOLYGONALFACE\(\(([^)]+)\)\)", text)
        ],
        dtype=np.int32,
    )
    return verts, faces


def _sample_tiffs(xs: np.ndarray, ys: np.ndarray) -> np.ndarray:
    flat_x = xs.ravel()
    flat_y = ys.ravel()
    zs = np.full(flat_x.shape, np.nan, dtype=np.float64)
    for path in (dgm_dir() / name for name in _TILES):
        if not path.is_file():
            raise FileNotFoundError(path)
        with rasterio.open(path) as src:
            data = src.read(1)
            rows, cols = rasterio.transform.rowcol(src.transform, flat_x, flat_y)
            rows = np.asarray(rows)
            cols = np.asarray(cols)
            inside = (rows >= 0) & (rows < src.height) & (cols >= 0) & (cols < src.width)
            if not np.any(inside):
                continue
            sampled = data[rows[inside], cols[inside]].astype(np.float64)
            ok = _elevation_ok(sampled)
            target = zs[inside]
            target[ok] = sampled[ok]
            zs[inside] = target
    return zs.reshape(xs.shape)


def _summarise(label: str, dz: np.ndarray) -> None:
    abs_dz = np.abs(dz)
    log.info(
        "%s  n=%d  mean=%+.3f  med=%+.3f  mae=%.3f  p95|d|=%.3f  max|d|=%.3f",
        label,
        dz.size,
        float(np.mean(dz)),
        float(np.median(dz)),
        float(np.mean(abs_dz)),
        float(np.percentile(abs_dz, 95)),
        float(np.max(abs_dz)),
    )
    for lim in (0.1, 0.5, 1.0, 2.0, 5.0):
        log.info("  |d| > %.1fm: %d (%.1f%%)", lim, int(np.sum(abs_dz > lim)), 100.0 * float(np.mean(abs_dz > lim)))


def main() -> None:
    if not IFC.is_file():
        raise FileNotFoundError(f"missing {IFC} — run example_terrain.py --bimfabrik first")

    min_x, min_y, max_x, max_y = _BBOX
    xs = np.linspace(min_x, max_x, _N)
    ys = np.linspace(min_y, max_y, _N)
    xx, yy = np.meshgrid(xs, ys)

    t0 = time.perf_counter()
    verts, faces = _parse_terrain_ifc(IFC)
    log.info("ifc: %.3fs  %d verts  %d faces", time.perf_counter() - t0, len(verts), len(faces))

    t0 = time.perf_counter()
    interp = LinearNDInterpolator(verts[:, :2], verts[:, 2])
    z_mesh = np.asarray(interp(xx, yy), dtype=np.float64)
    log.info("mesh interp: %.3fs  (Delaunay on %d IFC verts)", time.perf_counter() - t0, len(verts))

    t0 = time.perf_counter()
    z_tif = _sample_tiffs(xx, yy)
    log.info("tif sample: %.3fs", time.perf_counter() - t0)

    both = _elevation_ok(z_tif) & _elevation_ok(z_mesh)
    only_tif = _elevation_ok(z_tif) & ~_elevation_ok(z_mesh)
    only_mesh = _elevation_ok(z_mesh) & ~_elevation_ok(z_tif)
    neither = ~_elevation_ok(z_tif) & ~_elevation_ok(z_mesh)
    log.info(
        "grid %d×%d  both=%d  tif_only=%d  mesh_only=%d  neither=%d",
        _N,
        _N,
        int(np.sum(both)),
        int(np.sum(only_tif)),
        int(np.sum(only_mesh)),
        int(np.sum(neither)),
    )
    log.info(
        "z tif  %.2f…%.2f   z mesh  %.2f…%.2f",
        float(np.nanmin(z_tif)),
        float(np.nanmax(z_tif)),
        float(np.nanmin(z_mesh)),
        float(np.nanmax(z_mesh)),
    )

    dz = z_mesh - z_tif
    _summarise("mesh − tif (all both)", dz[both])
    interior = both & (xx > min_x + 5) & (xx < max_x - 5) & (yy > min_y + 5) & (yy < max_y - 5)
    _summarise("mesh − tif (interior >5 m from bbox)", dz[interior])

    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    np.savez_compressed(
        OUTPUT,
        xs=xs,
        ys=ys,
        z_tif=z_tif.astype(np.float32),
        z_mesh=z_mesh.astype(np.float32),
        dz=(dz).astype(np.float32),
        both=both,
    )
    log.info("wrote %s (%.2f MB)", OUTPUT, OUTPUT.stat().st_size / 1e6)


if __name__ == "__main__":
    main()
