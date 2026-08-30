"""One CityGML building → two IFCs with a CAD presentation layer.

Writes a mesh block and a typed-entity file. Both use
`cad_layer=True` (`IfcPresentationLayerWithStyle` `_BIM_Stadtmodell`).
`cad_layer` is off unless you pass `True`.
"""

from __future__ import annotations

import time

from bimfabrikhh_core_rs import gml_to_ifc, parse_gml
from bimfabrikhh_core_rs.attribute_mapping import specs
from bimfabrikhh_core_rs.ifc_types import hamburg_ifc_types

from envpaths import FIXTURES, ROOT
from logconfig import ENABLED, ifc_created, log

GML = FIXTURES / "lod2_33410_5652_2_sn.gml"
OUTPUT = ROOT / "output"
BUILDING_ID = "DESNATPU1000C1qE"

JOBS = (
    ("mesh", f"one_building_block_layer_{BUILDING_ID}.ifc", None),
    ("typed", f"one_building_entities_layer_{BUILDING_ID}.ifc", hamburg_ifc_types()),
)


def main() -> None:
    if not GML.is_file():
        raise FileNotFoundError(GML)
    OUTPUT.mkdir(parents=True, exist_ok=True)

    log.debug(f"GML:    {GML} ({GML.stat().st_size / 1e6:.1f} MB)")
    log.debug(f"filter: {BUILDING_ID}")
    log.debug("cad_layer: _BIM_Stadtmodell")

    t0 = time.perf_counter()
    buildings = parse_gml(str(GML), building_id_filter=BUILDING_ID, progress=ENABLED)
    log.debug(f"parse_gml: {len(buildings)} building(s) in {time.perf_counter() - t0:.2f}s")
    if not buildings:
        raise SystemExit("no buildings parsed")
    b = buildings[0]
    log.debug(
        f"  {b['id']}  gml_name={b.get('gml_name')!r}  lod={b['lod']!r}  "
        f"surfaces={len(b['boundaries'])}  attrs={b['attributes']}"
    )

    for mode, name, ifc_types in JOBS:
        ifc_path = OUTPUT / name
        log.debug(f"mode: {mode}  ->  {ifc_path.name}")
        t1 = time.perf_counter()
        written = gml_to_ifc(
            str(GML),
            str(ifc_path),
            building_id_filter=BUILDING_ID,
            mode=mode,
            project_name="Dresden_Sachsen_LoD2",
            site_name="Dresden_Site",
            epsg=25833,
            ifc_types=ifc_types,
            psets=specs(),
            cad_layer=True,
            progress=ENABLED,
        )
        elapsed = time.perf_counter() - t1
        log.debug(f"gml_to_ifc ({mode}): {elapsed:.2f}s")
        ifc_created(written, elapsed)


if __name__ == "__main__":
    main()
