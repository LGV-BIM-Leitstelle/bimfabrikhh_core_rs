# bimfabrikhh_core_rs

[![PyPI version](https://badge.fury.io/py/bimfabrikhh-core-rs.svg)](https://pypi.org/project/bimfabrikhh-core-rs/)
[![Python 3.11+](https://img.shields.io/badge/python-3.11+-blue.svg)](https://www.python.org/downloads/)
[![License](https://img.shields.io/badge/license-LGPL--2.1-blue.svg)](LICENSE)

Parse CityGML 1.0 / 2.0 and CityJSON 1.1 and write IFC4 STEP from Python.

## About This Project

**bimfabrikhh_core_rs** is developed and maintained by **Freie und Hansestadt Hamburg, Landesbetrieb Geoinformation und
Vermessung (LGV)**, **BIM-Leitstelle**

This library is part of [BIMFabrikHH](https://github.com/LGV-BIM-Leitstelle/BIMFabrikHH_core). It converts urban
building models to IFC4 STEP.

This project is in an **early stage of development**. The API and features will keep evolving.

**Author:** Ahmed Salem (BIM-Leitstelle, LGV Hamburg)

## Quick Install

```bash
pip install bimfabrikhh_core_rs
```

Optional pydantic helpers for property-set specs, IFC type maps, and owner identity:

```bash
pip install "bimfabrikhh_core_rs[psets]"
```

Wheels are published for Linux, Windows, and macOS. One wheel per platform works for Python 3.11+.

## Overview

The conversion API has no Python runtime dependencies and does not use `ifcopenshell`. Geometry is parsed and written in
Rust. You fetch the data; the package converts it. There is no HTTP inside the library.

- **CityGML 1.0 / 2.0** and **CityJSON 1.1** → one **IFC4** STEP file
- `mode="typed"` — one product per semantic surface (`IfcWall`, `IfcRoof`, `IfcSlab`, …)
- `mode="mesh"` — one `IfcBuildingElementProxy` per building
- Property sets, colours, CAD layer, and owner history are opt-in
- Coordinates stay in the CRS they arrive in; `epsg` only tags the file

## Public API

### Conversion

- **`gml_to_ifc(path, ifc_path, ...)`** — CityGML file or list of files → IFC
- **`cityjson_to_ifc(source, ifc_path, ...)`** — CityJSON path, list of paths, or document text → IFC
- **`write_ifc(products, path, ...)`** — IFC from product dicts you built yourself

### Parse

- **`parse_gml(path, ...)`** — CityGML → list of building dicts
- **`parse_cityjson(path, ...)`** — CityJSON file → building dicts
- **`parse_cityjson_text(text, ...)`** — CityJSON already in memory (after HTTP)

### Optional helpers (`bimfabrikhh_core_rs[psets]`)

- **`PsetSpec` / `Attr` / `Const` / `Quantity`** — declarative property-set rules
- **`hamburg_defaults()` / `attribute_mapping.specs()`** — Hamburg property names
- **`hamburg_ifc_types()` / `IfcTypeMap`** — surface type → IFC entity
- **`OwnerIdentity`** — `IfcPerson` / `IfcOrganization` / `IfcApplication`

## Key Features

- **No HTTP** — fetch tiles yourself; pass paths or CityJSON text
- **Typed or mesh** — per-surface products or one block per building
- **Declarative psets** — omit `psets=` and none are written
- **Deterministic GUIDs** — the same input produces a byte-identical IFC
- **Hamburg-ready** — optional type map, property mapping, `_BIM_Stadtmodell` layer, and Nullpunkt

## Complete Working Example

```python
from bimfabrikhh_core_rs import cityjson_to_ifc, gml_to_ifc, parse_cityjson_text

# CityGML tile → typed IFC (IfcWall / IfcRoof / IfcSlab per surface)
gml_to_ifc("tile.gml", "out.ifc", mode="typed", epsg=25833)

# CityJSON → one IfcBuildingElementProxy mesh per building
cityjson_to_ifc("page.city.json", "blocks.ifc", mode="mesh", epsg=25832)

# Inspect before writing
buildings = parse_cityjson_text(open("page.city.json").read())
print(len(buildings), buildings[0]["gml_name"])
```

Crop, colours, and Hamburg types:

```python
from bimfabrikhh_core_rs import cityjson_to_ifc
from bimfabrikhh_core_rs.attribute_mapping import specs
from bimfabrikhh_core_rs.ifc_types import hamburg_ifc_types

cityjson_to_ifc(
        "page.city.json",
        "hamburg.ifc",
        mode="typed",
        epsg=25832,
        bbox_epsg=(565012.0, 5933559.0, 566570.0, 5934571.0),
        basepoint=(565012.0, 5933559.0),
        ifc_types=hamburg_ifc_types(),
        psets=specs(),
        cad_layer=True,
)
```

`mode` is `"typed"` or `"mesh"`. Omit `ifc_types` and every product is `IfcBuildingElementProxy`. Omit `psets` and the
IFC has no property sets. `bbox_epsg` is `(min_x, min_y, max_x, max_y)` in the projected CRS. Reprojection is out of
scope — use `pyproj` on the caller side if you need to transform.

## Property sets

Pass `psets=` a list of specs describing rules rather than values. The spec is validated once and evaluated in Rust for
every building and surface.

```python
from bimfabrikhh_core_rs import cityjson_to_ifc
from bimfabrikhh_core_rs.psets import Attr, Const, PsetSpec, Quantity

building = PsetSpec(
        name="Pset_Objektinformation",
        scope="building",
        properties={
                "_IDEbene1": Const("Stadtmodell"),
                "_StadtmodellLoD": Attr("lod"),
                "_Dachform": Attr("roof_type", catalog="dachform"),
                "_GrundhoeheNN": Attr("extra:Grundhöhe NN"),
        },
)
surface = PsetSpec(
        name="BIMFabrikHH_Quantities",
        scope="surface",
        properties={
                "GrossArea": Quantity("area", decimals=4),
                "SurfaceType": Quantity("surface_type"),
        },
)

cityjson_to_ifc("page.city.json", "out.ifc", epsg=25832, psets=[building, surface])
```

| Value                                         | Reads                                                                                                                             |
|-----------------------------------------------|-----------------------------------------------------------------------------------------------------------------------------------|
| `Const(value)`                                | a fixed string                                                                                                                    |
| `Attr(key, catalog=…, decimals=…, default=…)` | a building field (`id`, `gml_name`, `lod`, `function`, `measured_height`, `storeys_above_ground`, `roof_type`, or `extra:<name>`) |
| `Quantity(key, decimals=…)`                   | surface geometry (`area`, `perimeter`, `tilt`, `surface_type`)                                                                    |

A missing source field omits that property. A plain dict works without pydantic.

## Other options

```python
from bimfabrikhh_core_rs import cityjson_to_ifc
from bimfabrikhh_core_rs.ifc_types import IfcTypeMap
from bimfabrikhh_core_rs.owner import OwnerIdentity

cityjson_to_ifc(
        "page.city.json",
        "out.ifc",
        epsg=25832,
        ifc_types=IfcTypeMap(RoofSurface="IfcRoof", WallSurface="IfcWall"),
        colors={"RoofSurface": (1.0, 0.0, 0.0)},
        owner=OwnerIdentity(organization="BIM-Leitstelle, LGV Hamburg"),
)
```

Default colours match BIMFabrikHH: roofs red, everything else yellow. `styles=False` writes no colour. Owner history
defaults to Ahmed Salem / BIM-Leitstelle, LGV Hamburg; `owner_history=False` skips it.

## Examples

Additional examples are available in
the [GitHub repository](https://github.com/LGV-BIM-Leitstelle/bimfabrikhh_core_rs/tree/main/examples).

## Links

- **PyPI**: https://pypi.org/project/bimfabrikhh-core-rs/
- **GitHub**: https://github.com/LGV-BIM-Leitstelle/bimfabrikhh_core_rs
- **Issues**: https://github.com/LGV-BIM-Leitstelle/bimfabrikhh_core_rs/issues

## License

This project is licensed under the GNU Lesser General Public License v2.1 (LGPL-2.1).

**Copyright (C) 2026 Freie und Hansestadt Hamburg, Landesbetrieb Geoinformation und Vermessung**
**BIM-Leitstelle, Ahmed Salem**
