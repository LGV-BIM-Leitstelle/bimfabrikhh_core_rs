"""Hamburg LoD3 CityGML → one IFC block per building.

Each building becomes a single IfcBuildingElementProxy mesh.
Default: TILE in hamburg_lod3.py. Set LOOP=True there for every tile.
"""

from __future__ import annotations

from bimfabrikhh_core_rs.crs import etrs89_utm

from pathlib import Path

from gml_tiles import convert_gml_tiles
from hamburg_lod3 import LOD3, TILE, LOOP

ROOT = Path(__file__).resolve().parents[1]


def main() -> None:
    convert_gml_tiles(
        LOD3,
        output=ROOT / "output",
        prefix="hamburg_lod3_blocks",
        mode="mesh",
        crs=etrs89_utm(32, vertical_datum="EPSG:7837"),
        loop=LOOP,
        tile=TILE,
    )


if __name__ == "__main__":
    main()
