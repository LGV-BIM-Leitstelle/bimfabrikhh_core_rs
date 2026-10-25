"""CityGML Kachel → typed IFC.

Each surface is its own product (IfcWall / IfcRoof / IfcSlab / …).
Default: first `*.gml` in fixtures/. Set loop=True for every tile.
"""

from __future__ import annotations

from bimfabrikhh_core_rs.crs import etrs89_utm
from bimfabrikhh_core_rs.ifc_types import hamburg_ifc_types

from envpaths import FIXTURES, ROOT
from gml_tiles import convert_gml_tiles


def main() -> None:
    convert_gml_tiles(
        FIXTURES,
        output=ROOT / "output",
        prefix="kachel_entities",
        mode="typed",
        crs=etrs89_utm(33, vertical_datum="EPSG:7837"),
        ifc_types=hamburg_ifc_types(),
    )


if __name__ == "__main__":
    main()
