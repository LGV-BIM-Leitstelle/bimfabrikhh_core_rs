"""Umring boreholes → IFC via Rust.

Same input as workflow ``005_bohrungen_generic``: ``boreholes_wfs.xml.gz``
for the Innenstadt / Alster Umring (~500 m × 500 m). ``BoreholeMLProcessor``
still parses it. Rust groups each borehole into one assembly and builds the solids.

    python examples/example_boreholes.py
"""

from __future__ import annotations

import gzip
import time
from pathlib import Path

from BIMFabrikHH_core import BoundingBoxParams
from BIMFabrikHH_core.apps.boreholes.generic.app import _cad_layer_name, _element_name, _element_name_groundwater
from BIMFabrikHH_core.apps.boreholes.processing import BoreholeMLProcessor
from BIMFabrikHH_core.core.geometry import basepoint_xyz
from BIMFabrikHH_core.core.georeferencing import CoordinateTransformer
from BIMFabrikHH_core.core.model_creator import output_coordinate_system, psets_to_wire
from BIMFabrikHH_core.data_models.boreholes import collect_borehole_psets, collect_groundwater_psets
from bimfabrikhh_core_rs import boreholes_to_ifc
from bimfabrikhh_core_rs.owner import defaults

from logconfig import ENABLED, ifc_created, log

HERE = Path(__file__).resolve().parent
FIXTURE = HERE / "boreholes_wfs.xml.gz"
OUTPUT = HERE.parent / "output" / "example_boreholes.ifc"

# Same Umring as examples/workflows/workflows.py (Innenstadt / Alster).
UMRING = BoundingBoxParams(min_x=9.9804, min_y=53.5513, max_x=9.9880, max_y=53.5558)
SOURCE_EPSG = 25832
# WFS heights are DHHN92, as in the workflow.
VERTICAL_EPSG = 5783


def _borehole_name(record) -> str:
    name = (record.aufschlussbezeichnung or "").strip()
    return name or record.borehole_id


def _solids(records) -> list[dict]:
    solids: list[dict] = []
    for record in records:
        borehole = record.borehole_id
        borehole_name = _borehole_name(record)
        for layer in record.layers:
            solids.append(
                {
                    "borehole": borehole,
                    "borehole_name": borehole_name,
                    "name": _element_name(record, layer),
                    "position": (record.easting, record.northing, layer.lower_height),
                    "height": layer.thickness,
                    "color": layer.visual_rgb,
                    "layer": _cad_layer_name(layer.din_color_name),
                    "psets": psets_to_wire(collect_borehole_psets(record, layer)),
                }
            )
        if record.groundwater is not None:
            solids.append(
                {
                    "borehole": borehole,
                    "borehole_name": borehole_name,
                    "name": _element_name_groundwater(record),
                    "kind": "square",
                    "position": (
                        record.easting,
                        record.northing,
                        record.ansatzhoehe_nn - record.groundwater.entry_depth,
                    ),
                    "psets": psets_to_wire(collect_groundwater_psets(record)),
                }
            )
    return solids


def main() -> None:
    if not FIXTURE.is_file():
        raise FileNotFoundError(FIXTURE)

    t0 = time.perf_counter()
    records = BoreholeMLProcessor().parse(gzip.decompress(FIXTURE.read_bytes()))
    n_layers = sum(len(record.layers) for record in records)
    n_water = sum(record.groundwater is not None for record in records)
    log.info(
        "parse: %d boreholes, %d layers, %d groundwater in %.3fs",
        len(records),
        n_layers,
        n_water,
        time.perf_counter() - t0,
    )
    if not records:
        raise SystemExit("no borehole records in fixture")

    transformer = CoordinateTransformer.from_source_to_output(SOURCE_EPSG, None, VERTICAL_EPSG)
    basepoint = basepoint_xyz(
        bbox_wgs84=(UMRING.min_x, UMRING.min_y, UMRING.max_x, UMRING.max_y),
        transformer=transformer,
    )
    solids = _solids(records)
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    log.debug("output: %s", OUTPUT)
    log.debug("solids: %d", len(solids))

    t1 = time.perf_counter()
    written = boreholes_to_ifc(
        solids,
        str(OUTPUT),
        project_name="Workflow 005_bohrungen_generic",
        site_name="Hamburg",
        crs=output_coordinate_system(transformer),
        owner=defaults(),
        basepoint=basepoint,
        progress=ENABLED,
    )
    ifc_created(written, time.perf_counter() - t1)


if __name__ == "__main__":
    main()
