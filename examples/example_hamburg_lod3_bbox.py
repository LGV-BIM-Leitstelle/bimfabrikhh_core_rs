"""Hamburg LoD3 CityGML → typed IFC, cropped with the same bbox as LoD2.

Tiles stay on disk. Only buildings that hit the box are written, one IFC.
"""

from __future__ import annotations

from pathlib import Path

from bimfabrikhh_core_rs.crs import etrs89_utm
from bimfabrikhh_core_rs.ifc_types import hamburg_ifc_types
from gml_tiles import convert_gml_crop
from hamburg_lod3 import BBOX_EPSG, BBOX_WGS84, LOD3
from logconfig import log

ROOT = Path(__file__).resolve().parents[1]


def main() -> None:
    log.debug(f"bbox WGS84: {BBOX_WGS84}")
    convert_gml_crop(
        LOD3,
        ifc_path=ROOT / "output" / "hamburg_lod3_bbox.ifc",
        mode="typed",
        crs=etrs89_utm(32, vertical_datum="EPSG:7837"),
        bbox_epsg=BBOX_EPSG,
        ifc_types=hamburg_ifc_types(),
        project_name="Hamburg_LoD3",
    )


if __name__ == "__main__":
    main()
