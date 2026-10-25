# bimfabrikhh_core_rs

[![PyPI version](https://badge.fury.io/py/bimfabrikhh-core-rs.svg)](https://pypi.org/project/bimfabrikhh-core-rs/)
[![Python 3.11+](https://img.shields.io/badge/python-3.11+-blue.svg)](https://www.python.org/downloads/)
[![License](https://img.shields.io/badge/license-LGPL--2.1-blue.svg)](LICENSE)

A Rust library with Python bindings: parse CityGML 1.0 / 2.0 and CityJSON 1.1 and write IFC4 STEP.

## About This Project

**bimfabrikhh_core_rs** is developed and maintained by **Freie und Hansestadt Hamburg, Landesbetrieb Geoinformation und
Vermessung (LGV)**, **BIM-Leitstelle**

This library is part of [BIMFabrikHH](https://github.com/LGV-BIM-Leitstelle/BIMFabrikHH_core). The core is implemented in **Rust**; **Python bindings** expose the conversion API. It converts urban building models to IFC4 STEP.

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

Wheels are published for Linux x86_64, Windows x64, and macOS Apple Silicon. One wheel per platform works for Python 3.11+.

## Overview

The conversion API has no Python runtime dependencies and does not use `ifcopenshell`. Geometry is parsed and written in
Rust. You fetch the data; the package converts it. There is no HTTP inside the library.

- **CityGML 1.0 / 2.0** and **CityJSON 1.1** → one **IFC4** STEP file
- `mode="typed"` — one product per semantic surface (`IfcWall`, `IfcRoof`, `IfcSlab`, …)
- `mode="mesh"` — one `IfcBuildingElementProxy` per building
- Property sets, colours, CAD layer, and owner history are opt-in
- Coordinates stay in the CRS they arrive in, unless you pass `transform=` (e.g. pyproj); `crs` tags the file
  (omit it for no georeferencing)

## Public API

### Conversion

- **`gml_to_ifc(path, ifc_path, ...)`** — CityGML file or list of files → IFC
- **`cityjson_to_ifc(source, ifc_path, ...)`** — CityJSON path, list of paths, or document text → IFC
- **`write_ifc(products, path, ...)`** — IFC from product dicts you built yourself
- **`trees_to_ifc(trees, ifc_path, ...)`** — trunk + crown meshes → IFC (TreesGenericApp path). Optional ``drape_vertices`` / ``drape_faces`` sets ``z = 0`` from the DGM triangle (else nearest vertex).
- **`terrain_to_ifc(vertices, faces, ifc_path, ...)`** — triangulated DGM → IFC (TerrainGenericApp write path)

### Parse

- **`parse_gml(path, ...)`** — CityGML → list of building dicts
- **`parse_cityjson(path, ...)`** — CityJSON file → building dicts
- **`parse_cityjson_text(text, ...)`** — CityJSON already in memory (after HTTP)
- **`declared_crs(path)`** — EPSG codes a CityGML (`srsName`) or CityJSON (`referenceSystem`) file declares

### Optional helpers (`bimfabrikhh_core_rs[psets]`)

- **`PsetSpec` / `Attr` / `Const` / `Quantity`** — declarative property-set rules
- **`hamburg_defaults()` / `attribute_mapping.specs()` / `tree_mapping.specs()` / `terrain_mapping.specs()`** — Hamburg property names
- **`hamburg_ifc_types()` / `IfcTypeMap`** — surface type → IFC entity
- **`OwnerIdentity` / `owner.defaults()`** — FILE_NAME + `IfcPerson` / `IfcOrganization` / `IfcApplication`

### Coordinate system (`bimfabrikhh_core_rs.crs`, no dependencies)

- **`etrs89_utm(zone, vertical_datum=...)` / `projected_crs(...)`** — `IfcProjectedCRS` values for `crs=`

## Key Features

- **No HTTP** — fetch tiles yourself; pass paths or CityJSON text
- **Typed or mesh** — per-surface products or one block per building
- **Declarative psets** — omit `psets=` and none are written
- **Deterministic GUIDs** — the same input produces a byte-identical IFC
- **Hamburg-ready** — optional type map, property mapping, `_BIM_Stadtmodell` layer, and Nullpunkt

## Complete Working Example

```python
from bimfabrikhh_core_rs import cityjson_to_ifc, gml_to_ifc, parse_cityjson_text
from bimfabrikhh_core_rs.crs import etrs89_utm

# CityGML tile → typed IFC (IfcWall / IfcRoof / IfcSlab per surface)
gml_to_ifc("tile.gml", "out.ifc", mode="typed", crs=etrs89_utm(33, vertical_datum="EPSG:7837"))

# CityJSON → one IfcBuildingElementProxy mesh per building
cityjson_to_ifc("page.city.json", "blocks.ifc", mode="mesh", crs=etrs89_utm(32, vertical_datum="EPSG:7837"))

# Inspect before writing
buildings = parse_cityjson_text(open("page.city.json").read())
print(len(buildings), buildings[0]["gml_name"])
```

Crop, colours, and Hamburg types:

```python
from bimfabrikhh_core_rs import cityjson_to_ifc
from bimfabrikhh_core_rs.attribute_mapping import specs
from bimfabrikhh_core_rs.crs import etrs89_utm
from bimfabrikhh_core_rs.ifc_types import hamburg_ifc_types

cityjson_to_ifc(
        "page.city.json",
        "hamburg.ifc",
        mode="typed",
        crs=etrs89_utm(32, vertical_datum="EPSG:7837"),
        bbox_epsg=(565012.0, 5933559.0, 566570.0, 5934571.0),
        basepoint=(565012.0, 5933559.0),
        ifc_types=hamburg_ifc_types(),
        psets=specs(),
        cad_layer=True,
)
```

`mode` is `"typed"` or `"mesh"`. Omit `ifc_types` and every product is `IfcBuildingElementProxy`. Omit `psets` and the
IFC has no property sets. `bbox_epsg` is `(min_x, min_y, max_x, max_y)` in the CRS of the source file.

Rust has no PROJ. To write the buildings in another CRS, pass `transform`: any callable `transform(xs, ys) -> (xs, ys)`,
e.g. pyproj's `Transformer.transform`. It is called once with the x/y of all kept buildings (after the `bbox_epsg`
filter); Z is not changed. `crs` and `basepoint` then describe the output CRS:

```python
from pyproj import Transformer

from bimfabrikhh_core_rs import gml_to_ifc
from bimfabrikhh_core_rs.crs import projected_crs

to_gk = Transformer.from_crs("EPSG:25832", "EPSG:10285", always_xy=True)
gml_to_ifc(
        "tile.gml",
        "gk.ifc",
        transform=to_gk.transform,
        crs=projected_crs("EPSG:10285", "ETRS89/DREF91/2016", map_projection="Transverse Mercator", map_zone="3",
                          vertical_datum="EPSG:7837"),
)
```

`crs` is the map CRS of the coordinates, written as `IfcProjectedCRS` + `IfcMapConversion`. Rust has no PROJ, so you
pass the fields: a dict from `bimfabrikhh_core_rs.crs` (`etrs89_utm(32, vertical_datum="EPSG:7837")` for UTM 32N with
DHHN2016 heights), any dict with the keys `name`, `geodetic_datum` and optionally `description`, `vertical_datum`,
`map_projection`, `map_zone`, or an object with `model_dump()` such as BIMFabrikHH_core's `CoordinateSystem`. Omit
`crs` and the IFC gets no georeferencing; leave `vertical_datum` out when the height system is unknown.

`gml_to_ifc` and `cityjson_to_ifc` check `crs` against what the files declare (`srsName`, e.g.
`urn:adv:crs:ETRS89_UTM32*DE_DHHN2016_NH`, or CityJSON `metadata.referenceSystem`). A missing `vertical_datum` is
filled from the files; a different `EPSG:` vertical datum raises, and so does a different horizontal code unless you
pass `transform`. Files that declare different CRS raise as well. Heights are never transformed.

```python
from bimfabrikhh_core_rs import declared_crs

declared_crs("tile.gml")
# {'srs_name': 'urn:adv:crs:ETRS89_UTM32*DE_DHHN2016_NH', 'horizontal_epsg': 25832, 'vertical_epsg': 7837}
```

Trees (same look as BIMFabrik `TreesGenericApp`; Python still prepares records):

```python
from bimfabrikhh_core_rs import trees_to_ifc
from bimfabrikhh_core_rs.crs import etrs89_utm
from bimfabrikhh_core_rs.tree_mapping import specs

trees_to_ifc(
    [
        {
            "name": "Baum_001",
            "position": (565100.0, 5933600.0, 0.0),
            "trunk_radius": 0.3,
            "trunk_height": 4.0,
            "crown_radius": 2.5,
            "detail": 1,
            "segments": 8,
            "attributes": {"baumnummer": "001", "gattung_deutsch": "Eiche", "art_baum": "Quercus robur"},
        }
    ],
    "trees.ifc",
    crs=etrs89_utm(32, vertical_datum="EPSG:7837"),
    psets=specs(),
    basepoint=(565100.0, 5933600.0),
    # drape_vertices=mesh.vertices,  # optional: lift z=0 from nearest DGM sample
)
```

`trunk_height` is the cylinder height, not the Stammhöhe. The crown sphere is centred on the cylinder top, so pass
Stammhöhe + `crown_radius` (BIMFabrikHH `TreeDimensions.trunk_cylinder_height`). The treetop then ends at
Stammhöhe + Kronendurchmesser, the published `_Baumhoehe`. In the example above the Stammhöhe is 1.5 m and the treetop
is at 6.5 m.

Terrain (Python still samples / Delaunay; Rust only writes IFC):

```python
from bimfabrikhh_core_rs import terrain_to_ifc
from bimfabrikhh_core_rs.crs import etrs89_utm
from bimfabrikhh_core_rs.terrain_mapping import specs

terrain_to_ifc(vertices, faces, "dgm.ifc", crs=etrs89_utm(32, vertical_datum="EPSG:7837"), psets=specs(), basepoint=(565000.0, 5933000.0))
```

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

cityjson_to_ifc("page.city.json", "out.ifc", psets=[building, surface])
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
from bimfabrikhh_core_rs.owner import defaults

cityjson_to_ifc(
        "page.city.json",
        "out.ifc",
        ifc_types=IfcTypeMap(RoofSurface="IfcRoof", WallSurface="IfcWall"),
        colors={"RoofSurface": (1.0, 0.0, 0.0)},
        owner=defaults(),
)
```

Default colours match BIMFabrikHH: roofs red, everything else yellow. `styles=False` writes no colour. Owner history
FILE_NAME author defaults to BIM-Leitstelle, LGV Hamburg; `owner_history=False` skips IfcOwnerHistory.

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
