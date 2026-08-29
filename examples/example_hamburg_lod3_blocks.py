"""Hamburg LoD3 CityGML → one IFC block per building.

Each building becomes a single IfcBuildingElementProxy mesh.
Default: TILE in hamburg_lod3.py. Set LOOP=True there for every tile.
"""

from __future__ import annotations

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
        epsg=25832,
        loop=LOOP,
        tile=TILE,
    )


if __name__ == "__main__":
    main()
