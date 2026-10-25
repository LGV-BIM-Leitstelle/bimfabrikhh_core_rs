"""20 random 2 km × 2 km umringe → one IFC each, for a chosen Hamburg LoD.

Folders come from ``.env``. Each crop is a random axis-aligned 2×2 km box
that contains at least one building. Empty water/edge umringe are skipped.
70 % of the umringe sit in the Hamburg city center (Altona–HafenCity–
Rotherbaum–St. Georg); the rest are elsewhere in the tile extent.
Models and ``umringe.geojson`` go into one folder.

Default is rust (``gml_to_ifc``). ``--generic`` uses ``CityGenericApp``
(LoD1 and LoD2 only). ``--both`` writes both IFCs for the same umringe.

    python example_random_2km.py --lod 1
    python example_random_2km.py --lod 2 --generic
    python example_random_2km.py --lod 1 --both
    python example_random_2km.py --lod 3 --n 20 --seed 1
"""

from __future__ import annotations

import argparse
import json
import os
import random
import time
from collections.abc import Callable
from pathlib import Path

from pyproj import Transformer

from bimfabrikhh_core_rs.crs import etrs89_utm
from bimfabrikhh_core_rs.ifc_types import hamburg_ifc_types

from envpaths import ROOT, lod1_dir, lod2_dir, lod3_dir
from gml_tiles import TileIndex, convert_gml_crop, folder_extent, tile_envelopes, tiles_in_bbox
from logconfig import log

EPSG = 25832
SIZE_M = 2000.0
CENTER_SHARE = 0.70
# Innenstadt belt: Altona — HafenCity — Rotherbaum — St. Georg (~8×6 km).
CENTER_EPSG = (562000.0, 5932000.0, 570000.0, 5938000.0)
_TO_WGS84 = Transformer.from_crs(f"EPSG:{EPSG}", "EPSG:4326", always_xy=True)
_LODS: dict[str, tuple[str, Callable[[], Path]]] = {
    "1": ("lod1", lod1_dir),
    "2": ("lod2", lod2_dir),
    "3": ("lod3", lod3_dir),
}


def _intersect(
    a: tuple[float, float, float, float],
    b: tuple[float, float, float, float],
) -> tuple[float, float, float, float]:
    return (max(a[0], b[0]), max(a[1], b[1]), min(a[2], b[2]), min(a[3], b[3]))


def _centroid_in(box: tuple[float, float, float, float], clip: tuple[float, float, float, float]) -> bool:
    cx, cy = (box[0] + box[2]) / 2, (box[1] + box[3]) / 2
    return clip[0] <= cx <= clip[2] and clip[1] <= cy <= clip[3]


def _iter_umringe(
    folder: Path,
    envelopes: TileIndex,
    *,
    size_m: float,
    rng: random.Random,
    clip: tuple[float, float, float, float] | None = None,
    exclude: tuple[float, float, float, float] | None = None,
    seen: set[tuple[int, int]] | None = None,
):
    extent = folder_extent(folder, envelopes)
    if clip is not None:
        extent = _intersect(extent, clip)
    minx, miny, maxx, maxy = extent
    span_x = maxx - minx - size_m
    span_y = maxy - miny - size_m
    if seen is None:
        seen = set()
    while True:
        x0 = minx if span_x <= 0 else rng.uniform(minx, minx + max(span_x, 0))
        y0 = miny if span_y <= 0 else rng.uniform(miny, miny + max(span_y, 0))
        box = (x0, y0, x0 + size_m, y0 + size_m)
        key = (round(x0), round(y0))
        if key in seen or not tiles_in_bbox(folder, box, envelopes):
            continue
        if exclude is not None and _centroid_in(box, exclude):
            continue
        seen.add(key)
        yield box


def _write_umringe(
    folder: Path,
    boxes: list[tuple[float, float, float, float]],
    *,
    lod: str,
    names: list[str],
    zones: list[str],
) -> None:
    """WGS84 GeoJSON (QGIS default) plus CSV/VRT in EPSG:25832 (open the .vrt)."""
    folder.mkdir(parents=True, exist_ok=True)
    features = []
    csv_lines = ["id,lod,ifc,zone,minx,miny,maxx,maxy,wkt"]
    for i, (box, name, zone) in enumerate(zip(boxes, names, zones), start=1):
        x0, y0, x1, y1 = box
        ring_ll = [
            list(_TO_WGS84.transform(x, y))
            for x, y in ((x0, y0), (x1, y0), (x1, y1), (x0, y1), (x0, y0))
        ]
        features.append(
            {
                "type": "Feature",
                "properties": {
                    "id": i,
                    "lod": lod,
                    "ifc": name,
                    "zone": zone,
                    "minx": x0,
                    "miny": y0,
                    "maxx": x1,
                    "maxy": y1,
                    "geometry_crs": "EPSG:4326",
                },
                "geometry": {"type": "Polygon", "coordinates": [ring_ll]},
            }
        )
        wkt = (
            f"POLYGON (({x0} {y0}, {x1} {y0}, {x1} {y1}, {x0} {y1}, {x0} {y0}))"
        )
        csv_lines.append(f'{i},{lod},{name},{zone},{x0},{y0},{x1},{y1},"{wkt}"')
    (folder / "umringe.geojson").write_text(
        json.dumps({"type": "FeatureCollection", "name": "umringe", "features": features}, indent=2),
        encoding="utf-8",
    )
    (folder / "umringe.csv").write_text("\n".join(csv_lines) + "\n", encoding="utf-8")
    (folder / "umringe.vrt").write_text(
        f"""<OGRVRTDataSource>
  <OGRVRTLayer name="umringe">
    <SrcDataSource relativeToVRT="1">umringe.csv</SrcDataSource>
    <GeometryType>wkbPolygon</GeometryType>
    <LayerSRS>EPSG:{EPSG}</LayerSRS>
    <Field name="id" type="Integer"/>
    <Field name="lod" type="String"/>
    <Field name="ifc" type="String"/>
    <Field name="zone" type="String"/>
    <Field name="minx" type="Real"/>
    <Field name="miny" type="Real"/>
    <Field name="maxx" type="Real"/>
    <Field name="maxy" type="Real"/>
    <GeometryField encoding="WKT" field="wkt"/>
  </OGRVRTLayer>
</OGRVRTDataSource>
""",
        encoding="utf-8",
    )


def _load_generic():
    try:
        from BIMFabrikHH_core import BoundingBoxParams, Component, Container, RequestParams
        from BIMFabrikHH_core.apps.city import CityGenericApp
        from BIMFabrikHH_core.config import setup_logging
    except ImportError as exc:
        raise ImportError(
            "CityGenericApp needs BIMFabrikHH_core in this environment "
            "(the BIMFabrik venv, not rust-only)."
        ) from exc
    setup_logging()
    return BoundingBoxParams, Component, Container, RequestParams, CityGenericApp


def _convert_generic(
    paths: list[Path],
    ifc_path: Path,
    box: tuple[float, float, float, float],
    *,
    title: str,
    cid: str,
) -> Path | None:
    BoundingBoxParams, Component, Container, RequestParams, CityGenericApp = _load_generic()
    lon0, lat0 = _TO_WGS84.transform(box[0], box[1])
    lon1, lat1 = _TO_WGS84.transform(box[2], box[3])
    req = RequestParams(
        bbox=BoundingBoxParams(
            min_x=min(lon0, lon1),
            min_y=min(lat0, lat1),
            max_x=max(lon0, lon1),
            max_y=max(lat0, lat1),
        ),
        containers=[
            Container(
                containerTitle=title,
                containerId=cid,
                components={"description": Component(title="Description", value=title)},
            )
        ],
    )
    # Absolute /mnt/... paths are treated as mount points; relative works on WSL and Windows.
    return CityGenericApp.from_gml_files(
        gml_files=[os.path.relpath(p) for p in paths],
        request_params=req,
        folder_path=None,
        output_path=ifc_path,
        basepoint_origin=(box[0], box[1]),
    )


def _print_table(
    rows: list[tuple[int, str, str, str, float, float, int, tuple[float, float, float, float]]],
    *,
    lod: str,
    mode: str,
    out: Path,
    skipped: int,
    wall_s: float,
) -> None:
    def _line(cells: list[str]) -> str:
        return "| " + " | ".join(cells) + " |"

    header = [
        f"{'#':>2}",
        f"{'app':8}",
        f"{'zone':7}",
        f"{'ifc':34}",
        f"{'s':>7}",
        f"{'MB':>6}",
        f"{'tiles':>5}",
        f"{'km²':>5}",
        f"{'ha':>6}",
        f"{'minx':>10}",
        f"{'miny':>10}",
        f"{'maxx':>10}",
        f"{'maxy':>10}",
    ]
    rule = "|-" + "-|-".join("-" * len(c) for c in header) + "-|"
    print()
    print(f"EPSG:{EPSG}")
    print(rule)
    print(_line(header))
    print(rule)
    last_i: int | None = None
    for i, app, zone, name, seconds, mb, n_tiles, box in rows:
        if last_i is not None and i != last_i:
            print(rule)
        last_i = i
        dx, dy = box[2] - box[0], box[3] - box[1]
        km2 = (dx * dy) / 1e6
        ha = (dx * dy) / 1e4
        print(
            _line(
                [
                    f"{i:2}",
                    f"{app:8}",
                    f"{zone:7}",
                    f"{name:34}",
                    f"{seconds:7.2f}",
                    f"{mb:6.2f}",
                    f"{n_tiles:5d}",
                    f"{km2:5.2f}",
                    f"{ha:6.1f}",
                    f"{box[0]:10.1f}",
                    f"{box[1]:10.1f}",
                    f"{box[2]:10.1f}",
                    f"{box[3]:10.1f}",
                ]
            )
        )
    print(rule)
    print()
    apps = list(dict.fromkeys(r[1] for r in rows))
    for app in apps:
        part = [r for r in rows if r[1] == app]
        times = [r[4] for r in part]
        mbs = [r[5] for r in part]
        box0 = part[0][7]
        n_center = sum(1 for r in part if r[2] == "center")
        km2 = ((box0[2] - box0[0]) * (box0[3] - box0[1])) / 1e6
        summary = (
            f"lod{lod}  {app}  {mode}  n={len(part)}  center={n_center}  "
            f"outer={len(part) - n_center}  skipped_empty={skipped}  "
            f"umring={km2:.2f} km² ({km2 * 100:.0f} ha)  "
            f"total={sum(times):.2f}s  wall={wall_s:.2f}s  "
            f"mean={sum(times) / len(part):.2f}s  "
            f"min={min(times):.2f}s  max={max(times):.2f}s  "
            f"{sum(mbs):.2f} MB  → {out}"
        )
        print(summary)
        print("-" * min(len(summary), 120))


def main() -> None:
    parser = argparse.ArgumentParser(description="20 random 2×2 km Hamburg crops → IFC folder")
    parser.add_argument("--lod", required=True, choices=("1", "2", "3"), help="which LoD folder")
    parser.add_argument("--n", type=int, default=20, help="number of models (default 20)")
    parser.add_argument("--seed", type=int, default=1, help="RNG seed")
    parser.add_argument(
        "--mode",
        choices=("mesh", "typed"),
        default="mesh",
        help="rust only: mesh = one proxy per building; typed = one product per surface",
    )
    which = parser.add_mutually_exclusive_group()
    which.add_argument("--rust", action="store_true", help="gml_to_ifc (default)")
    which.add_argument("--generic", action="store_true", help="CityGenericApp (LoD1/2)")
    which.add_argument("--both", action="store_true", help="rust and generic, same umringe")
    parser.add_argument("--out", type=Path, default=None, help="output folder")
    args = parser.parse_args()
    if args.n < 1:
        raise SystemExit("--n must be >= 1")
    if args.both:
        apps: tuple[str, ...] = ("rust", "generic")
    elif args.generic:
        apps = ("generic",)
    else:
        apps = ("rust",)
    if "generic" in apps and args.lod == "3":
        raise SystemExit("CityGenericApp is LoD1/LoD2 only. Use rust (default) for LoD3.")
    if "generic" in apps and args.mode == "typed":
        raise SystemExit("CityGenericApp is mesh only. Drop --mode typed or use rust.")

    prefix, folder_fn = _LODS[args.lod]
    folder = folder_fn()
    suffix = "" if apps == ("rust",) else "_" + "_".join(apps)
    out = args.out or (ROOT / "output" / f"random_2km_{prefix}{suffix}")
    out.mkdir(parents=True, exist_ok=True)

    rng = random.Random(args.seed)
    envelopes = tile_envelopes(folder)
    ifc_types = hamburg_ifc_types() if args.mode == "typed" else None
    if "generic" in apps:
        log.info("loading CityGenericApp (ifcopenshell / ifcfactory)…")
        _load_generic()
    n_center = round(args.n * CENTER_SHARE)
    log.info(
        "%d umringe  lod%s  %s  %s  center=%d outer=%d  → %s",
        args.n,
        args.lod,
        args.mode,
        "+".join(apps),
        n_center,
        args.n - n_center,
        out,
    )

    boxes: list[tuple[float, float, float, float]] = []
    names: list[str] = []
    zones: list[str] = []
    rows: list[tuple[int, str, str, str, float, float, int, tuple[float, float, float, float]]] = []
    skipped = 0
    tries = 0
    limit = max(args.n * 80, 80)
    wall0 = time.perf_counter()
    seen: set[tuple[int, int]] = set()
    center_it = _iter_umringe(
        folder, envelopes, size_m=SIZE_M, rng=rng, clip=CENTER_EPSG, seen=seen
    )
    outer_it = _iter_umringe(
        folder, envelopes, size_m=SIZE_M, rng=rng, exclude=CENTER_EPSG, seen=seen
    )
    need_center = n_center
    while len(boxes) < args.n:
        tries += 1
        if tries > limit:
            raise RuntimeError(f"only wrote {len(boxes)} of {args.n} models (too many empty crops)")
        if need_center > 0:
            box = next(center_it)
            zone = "center"
        else:
            box = next(outer_it)
            zone = "outer"
        i = len(boxes) + 1
        tiles = tiles_in_bbox(folder, box, envelopes)
        batch: list[tuple[int, str, str, str, float, float, int, tuple[float, float, float, float]]] = []
        empty = False
        for app in apps:
            name = (
                f"{i:02d}_{box[0]:.0f}_{box[1]:.0f}_{app}.ifc"
                if len(apps) > 1
                else f"{i:02d}_{box[0]:.0f}_{box[1]:.0f}.ifc"
            )
            log.info(
                "%02d/%02d  %s  %s  %s  tiles=%d  utm=%s",
                i,
                args.n,
                zone,
                app,
                name,
                len(tiles),
                tuple(round(v, 1) for v in box),
            )
            t0 = time.perf_counter()
            if app == "rust":
                written = convert_gml_crop(
                    folder,
                    ifc_path=out / name,
                    mode=args.mode,
                    crs=etrs89_utm(32, vertical_datum="EPSG:7837"),
                    bbox_epsg=box,
                    ifc_types=ifc_types,
                    project_name=f"random_2km_{prefix}_{i:02d}",
                    envelopes=envelopes,
                    allow_empty=True,
                )
            else:
                written = _convert_generic(
                    tiles,
                    out / name,
                    box,
                    title=f"random_2km_{prefix}_{i:02d}",
                    cid=f"random_2km_{i:02d}",
                )
            elapsed = time.perf_counter() - t0
            if written is None:
                empty = True
                break
            batch.append((i, app, zone, name, elapsed, written.stat().st_size / 1e6, len(tiles), box))
        if empty:
            skipped += 1
            continue
        boxes.append(box)
        names.append(batch[0][3])
        zones.append(zone)
        rows.extend(batch)
        if zone == "center":
            need_center -= 1
        _write_umringe(out, boxes, lod=args.lod, names=names, zones=zones)
        if len(boxes) >= args.n:
            break
    if rows:
        _print_table(rows, lod=args.lod, mode=args.mode, out=out, skipped=skipped, wall_s=time.perf_counter() - wall0)


if __name__ == "__main__":
    main()
