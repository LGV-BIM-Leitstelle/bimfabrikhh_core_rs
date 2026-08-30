"""Triangulated DGM → IFC via Rust (no ifcfactory).

Default: a small synthetic mesh (always runnable).
Pass ``--bimfabrik`` to mesh a 2 km × 2 km Hamburg crop with BIMFabrik
``extract_mesh_adaptive`` and write IFC with Rust.
"""

from __future__ import annotations

import argparse
import time
from pathlib import Path

from bimfabrikhh_core_rs import terrain_to_ifc
from bimfabrikhh_core_rs.owner import defaults
from bimfabrikhh_core_rs.terrain_mapping import specs

from envpaths import ROOT, dgm_dir
from logconfig import ENABLED, ifc_created, log

OUTPUT = ROOT / "output" / "example_terrain.ifc"
OUTPUT_2KM = ROOT / "output" / "example_terrain_2km.ifc"
_TILES = (
    "dgm1_32_565_9330_1_hh_2022.tif",
    "dgm1_32_565_9340_1_hh_2022.tif",
    "dgm1_32_566_9330_1_hh_2022.tif",
    "dgm1_32_566_9340_1_hh_2022.tif",
)
_BBOX = (565000.0, 5933000.0, 567000.0, 5935000.0)


def _synthetic() -> tuple[list[list[float]], list[list[int]]]:
    vertices = [
        [565000.0, 5933000.0, 4.0],
        [565050.0, 5933000.0, 4.2],
        [565050.0, 5933050.0, 5.1],
        [565000.0, 5933050.0, 4.3],
    ]
    faces = [[0, 1, 2], [0, 2, 3]]
    return vertices, faces


def _write(vertices, faces, path: Path, *, basepoint: tuple[float, float]) -> str:
    path.parent.mkdir(parents=True, exist_ok=True)
    t0 = time.perf_counter()
    written = terrain_to_ifc(
        vertices,
        faces,
        str(path),
        epsg=25832,
        psets=specs(),
        owner=defaults(),
        basepoint=basepoint,
        progress=ENABLED,
    )
    ifc_created(written, time.perf_counter() - t0)
    return written


def _bimfabrik_2km() -> None:
    from BIMFabrikHH_core.apps.terrain.processing import extract_mesh_adaptive

    tifs = [str(dgm_dir() / name) for name in _TILES]
    missing = [p for p in tifs if not Path(p).is_file()]
    if missing:
        raise FileNotFoundError("DGM tiles not found:\n" + "\n".join(missing))
    log.info("mesh: extract_mesh_adaptive 2 km × 2 km (%d tiles)", len(tifs))
    t0 = time.perf_counter()
    mesh = extract_mesh_adaptive(
        tifs,
        min_points=1000,
        importance_threshold=0.1,
        bbox_utm=_BBOX,
        move_to_origin=False,
    )
    log.info(
        "mesh: %.3fs  %d vertices  %d faces",
        time.perf_counter() - t0,
        len(mesh.vertices),
        len(mesh.faces),
    )
    if mesh.is_empty():
        raise SystemExit("empty terrain mesh")
    _write(mesh.vertices, mesh.faces, OUTPUT_2KM, basepoint=(_BBOX[0], _BBOX[1]))


def main(*, bimfabrik: bool = False) -> None:
    if bimfabrik:
        _bimfabrik_2km()
        return
    log.info("synthetic 2-triangle DGM")
    vertices, faces = _synthetic()
    _write(vertices, faces, OUTPUT, basepoint=(vertices[0][0], vertices[0][1]))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="DGM mesh → IFC via Rust")
    parser.add_argument(
        "--bimfabrik",
        action="store_true",
        help="2 km × 2 km Hamburg crop via BIMFabrik extract_mesh_adaptive",
    )
    main(bimfabrik=parser.parse_args().bimfabrik)
