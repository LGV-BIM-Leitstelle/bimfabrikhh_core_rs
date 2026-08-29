"""All buildings in every CityGML Kachel → typed IFC.

Each surface is its own product (IfcWall / IfcRoof / IfcSlab / …).
One IFC file per GML in fixtures/.
"""

from __future__ import annotations

import time
from pathlib import Path

from bimfabrikhh_core_rs import gml_to_ifc
from bimfabrikhh_core_rs.ifc_types import hamburg_ifc_types

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "fixtures"
OUTPUT = ROOT / "output"
MODE = "typed"


def main() -> None:
    tiles = sorted(FIXTURES.glob("*.gml"))
    if not tiles:
        raise FileNotFoundError(f"no *.gml in {FIXTURES}")
    OUTPUT.mkdir(parents=True, exist_ok=True)

    print(f"mode: {MODE}  (one IFC product per surface)")
    print(f"tiles: {len(tiles)}")

    for gml in tiles:
        ifc_path = OUTPUT / f"kachel_entities_{gml.stem}.ifc"
        print(f"\nGML: {gml.name} ({gml.stat().st_size / 1e6:.1f} MB)")

        t0 = time.perf_counter()
        written = gml_to_ifc(
                str(gml),
                str(ifc_path),
                mode=MODE,
                project_name=gml.stem,
                site_name="Site",
                epsg=25833,
                ifc_types=hamburg_ifc_types(),
        )
        print(f"gml_to_ifc: {time.perf_counter() - t0:.2f}s")
        print(f"wrote {written} ({Path(written).stat().st_size / 1e6:.2f} MB)")


if __name__ == "__main__":
    main()
