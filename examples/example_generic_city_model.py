"""Hamburg LoD1 city model — same setup as BIMFabrik `example_generic_city_model.py`.

Parses the Hamburg LoD1 tiles, crops them with the same WGS84 bbox
(reprojected to EPSG:25832), and writes one IFC. Each building is a
single mesh block (`IfcBuildingElementProxy`), matching CityGenericApp.
"""

from __future__ import annotations

import time
from pathlib import Path

from bimfabrikhh_core_rs import gml_to_ifc

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "output"
ASSETS = ROOT.parent / "BIMFabrikHH_core" / "examples" / "assets"

# Same tiles as BIMFabrikHH_core/examples/city_model/generic/example_generic_city_model.py
GML_FILES = [
        ASSETS / "LoD1_32_549_5935_1_HH.xml",
        ASSETS / "LoD1_32_549_5936_1_HH.xml",
        ASSETS / "LoD1_32_549_5937_1_HH.xml",
        ASSETS / "LoD1_32_549_5938_1_HH.xml",
        ASSETS / "LoD1_32_563_5934_1_HH.xml",
        ASSETS / "LoD1_32_564_5934_1_HH.xml",
]

# Same WGS84 crop (lon/lat). Parser wants UTM — this is EPSG:4326 → 25832.
BBOX_WGS84 = (9.9647, 53.5531, 9.9711, 53.5564)
BBOX_EPSG = (563910.1036, 5934234.0317, 564329.0775, 5934606.9103)
EPSG = 25832


def main() -> None:
    missing = [p for p in GML_FILES if not p.is_file()]
    if missing:
        raise FileNotFoundError(
                "Hamburg LoD1 tiles not found (expected next to this repo):\n" + "\n".join(f"  {p}" for p in missing)
        )
    OUTPUT.mkdir(parents=True, exist_ok=True)
    ifc_path = OUTPUT / "example_generic_city_model.ifc"

    print(f"tiles: {len(GML_FILES)}")
    for p in GML_FILES:
        print(f"  {p.name} ({p.stat().st_size / 1e6:.1f} MB)")
    print(f"bbox WGS84: {BBOX_WGS84}")
    print(f"bbox EPSG:{EPSG}: {BBOX_EPSG}")
    print("mode: mesh  (one IFC block per building, one file)")

    t0 = time.perf_counter()
    written = gml_to_ifc(
            [str(p) for p in GML_FILES],
            str(ifc_path),
            mode="mesh",
            project_name="Citymodel_Container",
            site_name="Hamburg_Site",
            epsg=EPSG,
            bbox_epsg=BBOX_EPSG,
            basepoint=(BBOX_EPSG[0], BBOX_EPSG[1]),
    )
    print(f"gml_to_ifc: {time.perf_counter() - t0:.2f}s")
    print(f"wrote {written} ({Path(written).stat().st_size / 1e6:.2f} MB)")


if __name__ == "__main__":
    main()
