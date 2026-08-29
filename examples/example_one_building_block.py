"""One CityGML building → one IFC block (IfcBuildingElementProxy).

All surfaces of the building are merged into a single mesh product.
Default building: TU Dresden, Georg-Schumann-Bau.
"""

from __future__ import annotations

import time
from pathlib import Path

from bimfabrikhh_core_rs import gml_to_ifc, parse_gml

ROOT = Path(__file__).resolve().parents[1]
GML = ROOT / "fixtures" / "lod2_33410_5652_2_sn.gml"
OUTPUT = ROOT / "output"
BUILDING_ID = "DESNATPU1000C1qE"
MODE = "mesh"


def main() -> None:
    if not GML.is_file():
        raise FileNotFoundError(GML)
    OUTPUT.mkdir(parents=True, exist_ok=True)
    ifc_path = OUTPUT / f"one_building_block_{BUILDING_ID}.ifc"

    print(f"GML:    {GML} ({GML.stat().st_size / 1e6:.1f} MB)")
    print(f"filter: {BUILDING_ID}")
    print(f"mode:   {MODE}  (one IFC product for the whole building)")

    t0 = time.perf_counter()
    buildings = parse_gml(str(GML), building_id_filter=BUILDING_ID)
    print(f"parse_gml: {len(buildings)} building(s) in {time.perf_counter() - t0:.2f}s")
    if not buildings:
        raise SystemExit("no buildings parsed")
    b = buildings[0]
    print(
            f"  {b['id']}  gml_name={b.get('gml_name')!r}  lod={b['lod']!r}  "
            f"surfaces={len(b['boundaries'])}  attrs={b['attributes']}"
    )

    t1 = time.perf_counter()
    written = gml_to_ifc(
            str(GML),
            str(ifc_path),
            building_id_filter=BUILDING_ID,
            mode=MODE,
            project_name="Dresden_Sachsen_LoD2",
            site_name="Dresden_Site",
            epsg=25833,
    )
    print(f"gml_to_ifc: {time.perf_counter() - t1:.2f}s")
    print(f"wrote {written} ({Path(written).stat().st_size / 1e6:.2f} MB)")


if __name__ == "__main__":
    main()
