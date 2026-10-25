"""Declarative property sets: pydantic defines the rules, Rust evaluates them.

Fetches Hamburg LoD2 CityJSON in memory (no page files).

Three points this demonstrates:
  1. Omit `psets` and the IFC has no property sets.
  2. Pass `hamburg_defaults()` if you want the Hamburg convention.
  3. A spec can be edited freely -- renamed properties, own catalogue, extra
     attributes, different number formatting.
"""

from __future__ import annotations

import re
import time
from collections import Counter
from pathlib import Path

from bimfabrikhh_core_rs import cityjson_to_ifc
from bimfabrikhh_core_rs.crs import etrs89_utm
from bimfabrikhh_core_rs.ifc_types import hamburg_ifc_types
from bimfabrikhh_core_rs.psets import (
    Attr,
    Const,
    PsetSpec,
    Quantity,
    hamburg_defaults,
)

from hamburg_lod2 import BBOX_EPSG, fetch_cityjson_pages
from logconfig import ENABLED, ifc_created, log

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "output"
CRS = etrs89_utm(32, vertical_datum="EPSG:7837")

PROP = re.compile(r"IFCPROPERTYSINGLEVALUE\('([^']*)',\$,IFC\w+\(([^)]*)\)")
PSET = re.compile(r"IFCPROPERTYSET\('[^']*',#\d+,'([^']*)'")


def convert(sources: list[str], name: str, psets=None) -> tuple[float, Path]:
    out = OUTPUT / name
    start = time.perf_counter()
    cityjson_to_ifc(
        sources,
        str(out),
        mode="typed",
        crs=CRS,
        bbox_epsg=BBOX_EPSG,
        project_name="Hamburg LoD2 pset spec",
        psets=psets,
        ifc_types=hamburg_ifc_types(),
        progress=ENABLED,
    )
    elapsed = time.perf_counter() - start
    ifc_created(out, elapsed)
    return elapsed, out


def fingerprint(path: Path) -> tuple[Counter, Counter]:
    text = path.read_text(encoding="utf-8", errors="replace")
    return Counter(PROP.findall(text)), Counter(PSET.findall(text))


def custom_specs() -> list[PsetSpec]:
    """A house convention that differs from the Hamburg default on every axis."""
    return [
        PsetSpec(
            name="Pset_MyOffice_Building",
            scope="building",
            properties={
                # Renamed, English property names instead of the _Xxx convention.
                "Discipline": Const("Stadtmodell"),
                "SourceId": Attr("id"),
                "LevelOfDetail": Attr("lod"),
                # Own catalogue instead of the built-in Objektartenkatalog.
                "RoofShape": Attr(
                    "roof_type",
                    catalog={"1000": "flat", "2100": "mono-pitch", "3100": "gable"},
                    default="other",
                ),
                # Fixed precision rather than the source precision.
                "HeightM": Attr("measured_height", decimals=1, default="0.0"),
                # Absent for most buildings, so it is simply dropped there.
                "Storeys": Attr("storeys_above_ground"),
            },
        ),
        PsetSpec(
            name="Pset_MyOffice_Surface",
            scope="surface",
            properties={
                "Kind": Quantity("surface_type"),
                "AreaM2": Quantity("area", decimals=2),
                # No perimeter and no tilt, so Rust never computes them.
            },
        ),
    ]


def main() -> None:
    sources = fetch_cityjson_pages()
    log.debug(f"{len(sources)} CityJSON page(s) in memory\n")

    t_none, f_none = convert(sources, "pset_none.ifc")
    t_hh, f_hh = convert(sources, "pset_hamburg.ifc", psets=hamburg_defaults())
    t_custom, f_custom = convert(sources, "pset_spec_custom.ifc", psets=custom_specs())

    p_none, s_none = fingerprint(f_none)
    p_hh, s_hh = fingerprint(f_hh)
    p_custom, s_custom = fingerprint(f_custom)

    log.debug("1) omit psets: no property sets")
    log.debug(f"   no psets     {t_none * 1000:7.0f} ms   {sum(p_none.values()):6} properties  {dict(s_none)}")

    log.debug("\n2) pass hamburg_defaults() explicitly")
    log.debug(f"   hamburg      {t_hh * 1000:7.0f} ms   {sum(p_hh.values()):6} properties  {dict(s_hh)}")

    log.debug("\n3) a custom convention")
    log.debug(f"   custom       {t_custom * 1000:7.0f} ms   {sum(p_custom.values()):6} properties")
    log.debug(f"   pset names:  {sorted(s_custom)}")
    sample = sorted(n for n, _ in p_custom)
    log.debug(f"   properties:  {sorted(set(sample))}")

    # Validation happens once, in Python, with a readable message.
    log.debug("\n4) a typo is caught before any conversion starts")
    try:
        PsetSpec(name="Bad", properties={"X": Attr("mesured_height")})
    except Exception as exc:
        first = str(exc).splitlines()
        log.debug("   " + "\n   ".join(first[:4]))


if __name__ == "__main__":
    main()
