"""Guided DGM (land-use parts) → one IFC via Rust ``terrain_parts_to_ifc``.

Self-contained: needs only ``bimfabrikhh_core_rs`` + the stdlib. It does **not**
import ``BIMFabrikHH_core`` and makes **no** network requests.

Why synthetic parts? The live guided pipeline — GeoTIFF sampling, the ALKIS
``Nutzung`` fetch from the Hamburg OAF, and the land-use split — lives in
``BIMFabrikHH_core`` (rasterio / scipy / shapely + HTTP). The Rust crate only
owns the *write* step: it turns already-split ``(name, vertices, faces)`` parts
into one IFC, each part its own styled ``IfcBuildingElementProxy`` contained in
the site, with per-part colour, CAD layer and property sets.

So this example builds a handful of pre-split parts by hand (a DGM base plus a
Fahrbahn strip, a Weg, a water patch and a Parcels block) and writes them with
``terrain_parts_to_ifc``. In production the parts come from
``TerrainRustApp.from_geotiffs(..., guide_from_oaf=True)`` instead — same Rust
write call, real geometry.
"""

from __future__ import annotations

import time
from pathlib import Path
from typing import Dict, List, Tuple

from bimfabrikhh_core_rs import terrain_parts_to_ifc
from bimfabrikhh_core_rs.owner import defaults

from envpaths import ROOT
from logconfig import ENABLED, ifc_created, log

OUTPUT = ROOT / "output" / "example_terrain_guided.ifc"

# Lower-left anchor (EPSG:25832) shared by every part → one local origin.
_ORIGIN = (565000.0, 5933000.0)

Vertices = List[List[float]]
Faces = List[List[int]]

# BIM.Hamburg hyperlink, plain wire dict (no pydantic needed).
_HYPERLINK: Dict[str, str] = {
    "_Hyperlink_001": "www.bim.hamburg.de",
    "_Hyperlink_001_Bemerkung": "Link zur Homepage von BIM.Hamburg",
}


def _grid(x0: float, y0: float, w: float, h: float, nx: int, ny: int, z: float) -> Tuple[Vertices, Faces]:
    """A triangulated rectangular patch with a gentle east-west slope."""
    vertices: Vertices = []
    for j in range(ny + 1):
        for i in range(nx + 1):
            x = x0 + w * i / nx
            y = y0 + h * j / ny
            vertices.append([x, y, z + 0.01 * i])
    faces: Faces = []
    stride = nx + 1
    for j in range(ny):
        for i in range(nx):
            a = j * stride + i
            b = a + 1
            c = a + stride
            d = c + 1
            faces.append([a, b, d])
            faces.append([a, d, c])
    return vertices, faces


def _objektinformation(idebene2: str, idebene3: str, bemerkung: str) -> Dict[str, str]:
    return {
        "_IDEbene1": "Gelaende",
        "_IDEbene2": idebene2,
        "_IDEbene3": idebene3,
        "_ArtDGM": "Netz",
        "_DatenHerkunft": "SDP",
        "_Bemerkung": bemerkung,
    }


def _part(
    name: str,
    grid: Tuple[Vertices, Faces],
    *,
    color: Tuple[int, int, int],
    layer: str,
    idebene2: str,
    idebene3: str,
    bemerkung: str,
) -> Dict[str, object]:
    vertices, faces = grid
    return {
        "name": name,
        "vertices": vertices,
        "faces": faces,
        "color": color,
        "layer": layer,
        "psets": {
            "Pset_Objektinformation": _objektinformation(idebene2, idebene3, bemerkung),
            "Pset_Hyperlink": _HYPERLINK,
        },
    }


def _parts() -> List[Dict[str, object]]:
    x0, y0 = _ORIGIN
    # A 200 m × 200 m tile split into land-use bands (colours / layers match the
    # BIMFabrikHH_core guided DGM).
    return [
        _part(
            "DGM",
            _grid(x0, y0, 200.0, 120.0, 20, 12, 4.0),
            color=(220, 40, 40),
            layer="_BIM_DGM_Gelaende",
            idebene2="Erdoberflaeche",
            idebene3="Erdoberflaeche",
            bemerkung="Leftover DGM (kein Nutzung-Polygon)",
        ),
        _part(
            "DGM_Strassenverkehr_Fahrbahn",
            _grid(x0, y0 + 120.0, 200.0, 30.0, 20, 3, 4.1),
            color=(55, 55, 60),
            layer="_BIM_DGM_Fahrbahn",
            idebene2="Strassenverkehr",
            idebene3="Fahrbahn",
            bemerkung="ALKIS Tatsaechliche Nutzung",
        ),
        _part(
            "DGM_Weg",
            _grid(x0, y0 + 150.0, 200.0, 15.0, 20, 2, 4.15),
            color=(230, 120, 40),
            layer="_BIM_DGM_Weg",
            idebene2="Weg",
            idebene3="Weg",
            bemerkung="ALKIS Tatsaechliche Nutzung",
        ),
        _part(
            "DGM_Stehendes_Gewaesser",
            _grid(x0, y0 + 165.0, 90.0, 35.0, 9, 4, 3.6),
            color=(120, 170, 210),
            layer="_BIM_DGM_StehendesGewaesser",
            idebene2="Stehendes Gewaesser",
            idebene3="Stehendes Gewaesser",
            bemerkung="Gewaesserflaeche (flach)",
        ),
        _part(
            "DGM_Parcels",
            _grid(x0 + 90.0, y0 + 165.0, 110.0, 35.0, 11, 4, 4.2),
            color=(166, 140, 100),
            layer="_BIM_DGM_Parcels",
            idebene2="Parcels",
            idebene3="Parcels",
            bemerkung="Siedlung / Unland (zusammengefasst)",
        ),
    ]


def main() -> None:
    parts = _parts()
    n_faces = sum(len(p["faces"]) for p in parts)  # type: ignore[arg-type]
    log.info("guided DGM: %d parts, %d faces total", len(parts), n_faces)

    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    t0 = time.perf_counter()
    written = terrain_parts_to_ifc(
        parts,
        str(OUTPUT),
        project_name="DGM_Guided",
        site_name="Hamburg_Site",
        epsg=25832,
        basepoint=_ORIGIN,
        owner=defaults(),
        progress=ENABLED,
    )
    ifc_created(written, time.perf_counter() - t0)


if __name__ == "__main__":
    main()
