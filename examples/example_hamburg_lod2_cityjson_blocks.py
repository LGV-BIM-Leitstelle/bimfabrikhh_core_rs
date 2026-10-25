"""Hamburg LoD2 CityJSON → IFC blocks (one proxy per building).

HTTP lives in this example only. Pages stay in memory; nothing is written except the IFC.
"""

from __future__ import annotations

import time
from pathlib import Path

from bimfabrikhh_core_rs import cityjson_to_ifc, parse_cityjson_text
from bimfabrikhh_core_rs.attribute_mapping import specs
from bimfabrikhh_core_rs.crs import etrs89_utm

from hamburg_lod2 import BBOX_EPSG, BBOX_WGS84, fetch_cityjson_pages
from logconfig import ENABLED, ifc_created, log

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "output"


def main() -> None:
    OUTPUT.mkdir(parents=True, exist_ok=True)
    ifc_path = OUTPUT / "hamburg_lod2_cityjson_blocks.ifc"

    log.debug("mode: mesh  (one IfcBuildingElementProxy per building)")
    log.debug(f"crop: EPSG:25832 {BBOX_EPSG}")
    log.debug(f"bbox WGS84: {BBOX_WGS84}")

    t0 = time.perf_counter()
    pages = fetch_cityjson_pages()
    log.debug(f"fetched {len(pages)} page(s) in {time.perf_counter() - t0:.2f}s")
    if not pages:
        raise SystemExit("no CityJSON pages returned")

    t1 = time.perf_counter()
    sample = parse_cityjson_text(pages[0], bbox_epsg=BBOX_EPSG)
    log.debug(f"parse_cityjson_text: {len(sample)} building(s) in {time.perf_counter() - t1:.2f}s")
    for b in sample[:3]:
        log.debug(f"  {b['id']}  name={b.get('gml_name')!r}  surfaces={len(b['boundaries'])}")

    t2 = time.perf_counter()
    written = cityjson_to_ifc(
        pages,
        str(ifc_path),
        mode="mesh",
        project_name="Hamburg_LoD2_CityJSON",
        site_name="Hamburg_Site",
        crs=etrs89_utm(32, vertical_datum="EPSG:7837"),
        bbox_epsg=BBOX_EPSG,
        basepoint=(BBOX_EPSG[0], BBOX_EPSG[1]),
        psets=specs(),
        progress=ENABLED,
    )
    elapsed = time.perf_counter() - t2
    log.debug(f"cityjson_to_ifc: {elapsed:.2f}s")
    ifc_created(written, elapsed)


if __name__ == "__main__":
    main()
