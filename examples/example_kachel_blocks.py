"""CityGML Kachel → one IFC block per building.

Each CityGML building becomes a single IfcBuildingElementProxy mesh.
Default: first `*.gml` in fixtures/. Set loop=True for every tile.
"""

from __future__ import annotations

from bimfabrikhh_core_rs.crs import etrs89_utm

from envpaths import FIXTURES, ROOT
from gml_tiles import convert_gml_tiles


def main() -> None:
    convert_gml_tiles(
        FIXTURES,
        output=ROOT / "output",
        prefix="kachel_blocks",
        mode="mesh",
        crs=etrs89_utm(33, vertical_datum="EPSG:7837"),
    )


if __name__ == "__main__":
    main()
