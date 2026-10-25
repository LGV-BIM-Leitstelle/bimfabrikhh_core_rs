"""Export every CityGML tile in a Hamburg LoD folder (one IFC per file).

Folders come from ``.env`` (``BIMFABRIKHH_LOD1_GML_DIR`` / ``LOD2`` / ``LOD3``).

    python example_export_folder.py --lod 1
    python example_export_folder.py --lod 2
    python example_export_folder.py --lod 3
    python example_export_folder.py --lod all

Default ``--mode mesh`` writes one proxy per building. ``--mode typed`` is
per-surface and much larger (especially LoD3).
"""

from __future__ import annotations

import argparse
from collections.abc import Callable
from pathlib import Path

from bimfabrikhh_core_rs.crs import etrs89_utm
from bimfabrikhh_core_rs.ifc_types import hamburg_ifc_types

from envpaths import ROOT, lod1_dir, lod2_dir, lod3_dir
from gml_tiles import convert_gml_tiles
from logconfig import log

CRS = etrs89_utm(32, vertical_datum="EPSG:7837")

_LODS: dict[str, tuple[str, Callable[[], Path]]] = {
    "1": ("lod1", lod1_dir),
    "2": ("lod2", lod2_dir),
    "3": ("lod3", lod3_dir),
}


def export_lod(lod: str, *, mode: str) -> None:
    prefix, folder_fn = _LODS[lod]
    folder = folder_fn()
    output = ROOT / "output" / prefix
    log.info("lod%s: %s → %s  mode=%s", lod, folder, output, mode)
    convert_gml_tiles(
        folder,
        output=output,
        prefix=prefix,
        mode=mode,
        crs=CRS,
        ifc_types=hamburg_ifc_types() if mode == "typed" else None,
        site_name="Hamburg_Site",
        loop=True,
    )


def main() -> None:
    parser = argparse.ArgumentParser(description="Export all CityGML tiles in a Hamburg LoD folder")
    parser.add_argument(
        "--lod",
        required=True,
        choices=("1", "2", "3", "all"),
        help="which LoD folder to export",
    )
    parser.add_argument(
        "--mode",
        choices=("mesh", "typed"),
        default="mesh",
        help="mesh = one proxy per building (default); typed = one product per surface",
    )
    args = parser.parse_args()
    lods = ("1", "2", "3") if args.lod == "all" else (args.lod,)
    for lod in lods:
        export_lod(lod, mode=args.mode)


if __name__ == "__main__":
    main()
