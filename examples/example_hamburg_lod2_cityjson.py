"""Hamburg LoD2 CityJSON → IFC (typed surfaces).

HTTP lives in this example only — not in the `bimfabrikhh_core_rs` package.
"""

from __future__ import annotations

import json
import time
import urllib.parse
import urllib.request
from pathlib import Path

from bimfabrikhh_core_rs import cityjson_to_ifc, parse_cityjson_text
from bimfabrikhh_core_rs.ifc_types import hamburg_ifc_types

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "output"

API = "https://api.hamburg.de/datasets/v1/lod2_hamburg/collections/building/items"

# WGS84 (lon, lat) crop → axis-aligned EPSG:25832 envelope of the four corners.
BBOX_WGS84 = (9.9814, 53.5469, 10.0047, 53.5558)
BBOX_EPSG = (565012.2860, 5933559.4141, 566569.7024, 5934571.0369)
CRS_CITYJSON = "http://www.opengis.net/def/crs/EPSG/0/5555"
# Hamburg OAF max page size (higher values are silently capped).
PAGE = 10000
PAGE_GLOB = "hamburg_lod2_crop_l10000_*.city.json"


def fetch_cityjson_pages(*, bbox=BBOX_WGS84, page: int = PAGE) -> list[Path]:
    """Download every OAF page into output/. Returns the written file paths."""
    OUTPUT.mkdir(parents=True, exist_ok=True)
    paths: list[Path] = []
    offset = 0
    while True:
        q = urllib.parse.urlencode(
                {
                        "f": "cityjson",
                        "limit": str(page),
                        "offset": str(offset),
                        "bbox": ",".join(str(v) for v in bbox),
                        "crs": CRS_CITYJSON,
                }
        )
        url = f"{API}?{q}"
        print(f"GET offset={offset} limit={page}")
        req = urllib.request.Request(url, headers={"Accept": "application/city+json"})
        with urllib.request.urlopen(req, timeout=300) as resp:
            text = resp.read().decode("utf-8")
        doc = json.loads(text)
        n = len(doc.get("CityObjects") or {})
        path = OUTPUT / f"hamburg_lod2_crop_l{page}_{offset:05d}.city.json"
        path.write_text(text, encoding="utf-8", newline="\n")
        paths.append(path)
        print(f"  {n} CityObjects → {path.name} ({path.stat().st_size / 1e3:.1f} kB)")
        if n == 0:
            break
        offset += page
        if n < page:
            break
    return paths


def existing_pages() -> list[Path]:
    pages = sorted(OUTPUT.glob(PAGE_GLOB))
    return [p for p in pages if p.stat().st_size > 0]


def main() -> None:
    OUTPUT.mkdir(parents=True, exist_ok=True)
    ifc_path = OUTPUT / "hamburg_lod2_cityjson.ifc"

    print(f"crop: EPSG:25832 {BBOX_EPSG}")
    print(f"bbox WGS84: {BBOX_WGS84}")

    pages = existing_pages()
    if pages:
        print(f"reuse {len(pages)} cached CityJSON page(s)")
    else:
        t0 = time.perf_counter()
        pages = fetch_cityjson_pages()
        print(f"fetched {len(pages)} page(s) in {time.perf_counter() - t0:.2f}s")

    t1 = time.perf_counter()
    buildings = []
    for page in pages:
        buildings.extend(parse_cityjson_text(page.read_text(encoding="utf-8"), bbox_epsg=BBOX_EPSG))
    print(f"parse_cityjson_text: {len(buildings)} building(s) in {time.perf_counter() - t1:.2f}s")
    for b in buildings[:5]:
        print(
                f"  {b['id']}  name={b.get('gml_name')!r}  lod={b['lod']!r}  "
                f"surfaces={len(b['boundaries'])}  attrs={b['attributes']}"
        )

    t2 = time.perf_counter()
    written = cityjson_to_ifc(
            [str(p) for p in pages],
            str(ifc_path),
            mode="typed",
            project_name="Hamburg_LoD2_CityJSON",
            site_name="Hamburg_Site",
            epsg=25832,
            bbox_epsg=BBOX_EPSG,
            basepoint=(BBOX_EPSG[0], BBOX_EPSG[1]),
            ifc_types=hamburg_ifc_types(),
    )
    print(f"cityjson_to_ifc: {time.perf_counter() - t2:.2f}s")
    print(f"wrote {written} ({Path(written).stat().st_size / 1e6:.2f} MB)")


if __name__ == "__main__":
    main()
