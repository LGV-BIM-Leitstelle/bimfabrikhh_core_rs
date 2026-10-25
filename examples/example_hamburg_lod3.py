"""Hamburg LoD3 CityGML → typed IFC.

Each surface is its own product. Default: TILE in hamburg_lod3.py.
Set LOOP=True there to convert every tile.
"""

from __future__ import annotations

from pathlib import Path

from bimfabrikhh_core_rs.crs import etrs89_utm
from bimfabrikhh_core_rs.ifc_types import hamburg_ifc_types

from gml_tiles import convert_gml_tiles
from hamburg_lod3 import LOD3, LOOP, TILE

ROOT = Path(__file__).resolve().parents[1]


def main() -> None:
    convert_gml_tiles(
        LOD3,
        output=ROOT / "output",
        prefix="hamburg_lod3_entities",
        mode="mesh",
        crs=etrs89_utm(32, vertical_datum="EPSG:7837"),
        ifc_types=hamburg_ifc_types(),
        loop=LOOP,
        tile=TILE,
    )


if __name__ == "__main__":
    main()
