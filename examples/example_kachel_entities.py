"""CityGML Kachel → typed IFC.

Each surface is its own product (IfcWall / IfcRoof / IfcSlab / …).
Default: first `*.gml` in fixtures/. Set loop=True for every tile.
"""

from __future__ import annotations

from pathlib import Path

from bimfabrikhh_core_rs.ifc_types import hamburg_ifc_types

from gml_tiles import convert_gml_tiles

ROOT = Path(__file__).resolve().parents[1]


def main() -> None:
    convert_gml_tiles(
        ROOT / "fixtures",
        output=ROOT / "output",
        prefix="kachel_entities",
        mode="typed",
        epsg=25833,
        ifc_types=hamburg_ifc_types(),
    )


if __name__ == "__main__":
    main()
