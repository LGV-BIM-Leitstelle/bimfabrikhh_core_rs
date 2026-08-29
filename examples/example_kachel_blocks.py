"""CityGML Kachel → one IFC block per building.

Each CityGML building becomes a single IfcBuildingElementProxy mesh.
Default: first `*.gml` in fixtures/. Set loop=True for every tile.
"""

from __future__ import annotations

from pathlib import Path

from gml_tiles import convert_gml_tiles

ROOT = Path(__file__).resolve().parents[1]


def main() -> None:
    convert_gml_tiles(
        ROOT / "fixtures",
        output=ROOT / "output",
        prefix="kachel_blocks",
        mode="mesh",
        epsg=25833,
    )


if __name__ == "__main__":
    main()
