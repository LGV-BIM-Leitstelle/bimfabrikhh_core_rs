"""Fetch Hamburg LoD2 CityJSON in memory. Writes no files."""

from __future__ import annotations

import json
import urllib.parse
import urllib.request

from logconfig import log

API = "https://api.hamburg.de/datasets/v1/lod2_hamburg/collections/building/items"
BBOX_WGS84 = (9.9814, 53.5469, 10.0047, 53.5558)
BBOX_EPSG = (565012.2860, 5933559.4141, 566569.7024, 5934571.0369)
CRS_CITYJSON = "http://www.opengis.net/def/crs/EPSG/0/5555"
PAGE = 10000


def fetch_cityjson_pages(*, bbox=BBOX_WGS84, page: int = PAGE) -> list[str]:
    """Download every OAF page and return the CityJSON documents as strings."""
    docs: list[str] = []
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
        log.debug(f"GET offset={offset} limit={page}")
        req = urllib.request.Request(url, headers={"Accept": "application/city+json"})
        with urllib.request.urlopen(req, timeout=300) as resp:
            text = resp.read().decode("utf-8")
        n = len(json.loads(text).get("CityObjects") or {})
        log.debug(f"  {n} CityObjects")
        if n == 0:
            break
        docs.append(text)
        offset += page
        if n < page:
            break
    return docs
