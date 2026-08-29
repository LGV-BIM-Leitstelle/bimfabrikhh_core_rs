# bimfabrikhh_core_rs

Parse **CityGML 1.0 / 2.0** and **CityJSON 1.1** and write **IFC4 STEP** text, implemented in
Rust and callable from Python.

The IFC is written as hand-rolled STEP text — no `ifcopenshell` dependency. Property sets are
optional: pass a [declarative spec](#property-sets) from Python, or omit `psets` and none are
written. No HTTP happens inside the package: you fetch the data, the package converts it.

---

## Requirements

| Component | Version              | Notes                                   |
|-----------|----------------------|-----------------------------------------|
| Rust      | 1.98.0 (1.63+ works) | install via [rustup](https://rustup.rs) |
| Python    | 3.11 or newer        | `requires-python = ">=3.11"`            |
| maturin   | 1.7 – 1.x            | the PEP 517 build backend               |
| pyo3      | 0.23                 | pulled in by cargo, nothing to do       |

The conversion API has no Python runtime dependencies. The optional
[`bimfabrikhh_core_rs.psets`](#property-sets) module needs pydantic:

```bash
pip install "bimfabrikhh_core_rs[psets]"
```

---

## Installation with maturin

### 1. Install the Rust toolchain

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
rustc --version
```

### 2. Create and activate a virtual environment

```bash
python3 -m venv ~/venvs/bimfabrikhh_core_rs
source ~/venvs/bimfabrikhh_core_rs/bin/activate
```

On Windows use `.\venvs\bimfabrikhh_core_rs\Scripts\activate` instead.

### 3. Install maturin

```bash
pip install "maturin>=1.7,<2"
```

### 4. Build and install into the active venv

```bash
cd bimfabrikhh_core_rs
maturin develop
```

`maturin develop` compiles the crate and installs it into whichever venv is active. It is the
normal command while developing.

For a build with optimizations — **much** faster on large tiles, and what you want for real
conversions — use:

```bash
maturin develop --release
```

### 5. Verify

```bash
python -c "import bimfabrikhh_core_rs; print(bimfabrikhh_core_rs.__all__)"
```

Expected:

```
['parse_gml', 'parse_cityjson', 'parse_cityjson_text', 'write_ifc', 'gml_to_ifc', 'cityjson_to_ifc', 'cityjson_to_ifc_blocks']
```

### Alternatives

An editable install through pip uses the same maturin backend:

```bash
pip install -e .
```

To produce a redistributable wheel instead of installing:

```bash
maturin build --release
# wheel lands in target/wheels/
```

### Rebuilding after a change

Editing any `.rs` file requires a rebuild; editing files under `python/` does not.

```bash
source ~/.cargo/env
source ~/venvs/bimfabrikhh_core_rs/bin/activate
maturin develop --release
```

If maturin cannot find your interpreter, point it at one explicitly with
`maturin develop -i $(which python)`, or export `VIRTUAL_ENV` before calling it.

---

## Quick start

```python
from bimfabrikhh_core_rs import cityjson_to_ifc_blocks, gml_to_ifc, parse_cityjson_text

# CityGML tile -> typed IFC (IfcWall / IfcRoof / IfcSlab per surface)
gml_to_ifc("tile.gml", "out.ifc", mode="typed", epsg=25833)

# CityJSON -> one IfcBuildingElementProxy mesh per building
cityjson_to_ifc_blocks(["page_00000.city.json"], "blocks.ifc", epsg=25832)

# Inspect before writing
buildings = parse_cityjson_text(open("page_00000.city.json").read())
print(len(buildings), buildings[0]["gml_name"])
```

---

## API

| Function                                        | Purpose                                                   |
|-------------------------------------------------|-----------------------------------------------------------|
| `parse_gml(path, ...)`                          | CityGML file to a list of building dicts                  |
| `parse_cityjson(path, ...)`                     | CityJSON file to building dicts                           |
| `parse_cityjson_text(text, ...)`                | CityJSON already in memory (post-HTTP)                    |
| `gml_to_ifc(gml_path, ifc_path, ...)`           | CityGML (one file or a list) to one IFC                   |
| `cityjson_to_ifc(source, ifc_path, ...)`        | CityJSON path, list of paths, or document text to one IFC |
| `cityjson_to_ifc_blocks(source, ifc_path, ...)` | Same, always mesh: one proxy per building                 |
| `write_ifc(products, path, ...)`                | IFC from product dicts you built yourself                 |

`mode` is `"typed"` or `"mesh"`. Typed gives one IFC product per semantic surface; mesh gives one
product per building. Product types come from `ifc_types=` — omit it and everything is
`IfcBuildingElementProxy`. `cityjson_to_ifc_blocks` is the mesh shortcut and takes no `mode`.

`bbox_epsg` crops to `(min_x, min_y, max_x, max_y)` in the projected CRS. `basepoint` sets the
IFC base point. The `epsg` default is **25833** on the CityGML entry points and **25832** on the
CityJSON ones, matching the usual Sachsen and Hamburg sources.

Reprojection is out of scope: coordinates are written in the CRS they arrive in, and `epsg` only
tags the file. Use `pyproj` on the caller side if you need to transform.

---

## Property sets

Pass `psets=` a list of specs describing *rules* rather than values. Omit it and the IFC has no
property sets — Rust does not fill any in. The spec is validated once by pydantic, crosses into
Rust once, and is then evaluated for every building and surface at native speed.

```python
from bimfabrikhh_core_rs import cityjson_to_ifc
from bimfabrikhh_core_rs.psets import Attr, Const, PsetSpec, Quantity

building = PsetSpec(
    name="Pset_Objektinformation",
    scope="building",
    properties={
        "_IDEbene1":       Const("Stadtmodell"),
        "_StadtmodellLoD": Attr("lod"),
        "_FunktionGebaeude": Attr("function", catalog="funktion", default="undefiniert"),
        "_RelativeHoehe":  Attr("measured_height", decimals=3, default="0.0"),
        "_Dachform":       Attr("roof_type", catalog="dachform", default="undefiniert"),
    },
)
surface = PsetSpec(
    name="BIMFabrikHH_Quantities",
    scope="surface",
    properties={
        "GrossArea":   Quantity("area", decimals=4),
        "SurfaceType": Quantity("surface_type"),
    },
)

cityjson_to_ifc("page.city.json", "out.ifc", epsg=25832, psets=[building, surface])
```

---

## IFC entity types

Same rule as psets: you pass the map, Rust applies it, and omitting `ifc_types` means no special
typing. Unmapped surfaces (and mesh products without a `"Building"` key) become
`IfcBuildingElementProxy`.

```python
from bimfabrikhh_core_rs import cityjson_to_ifc
from bimfabrikhh_core_rs.ifc_types import IfcTypeMap, hamburg_ifc_types

cityjson_to_ifc("page.city.json", "out.ifc", epsg=25832, ifc_types=hamburg_ifc_types())

cityjson_to_ifc(
    "page.city.json",
    "out.ifc",
    epsg=25832,
    ifc_types=IfcTypeMap(RoofSurface="IfcRoof", WallSurface="IfcWall"),
)
```

A plain `{"RoofSurface": "IfcRoof", ...}` dict works without pydantic. `hamburg_ifc_types()` is
the usual LoD2 map (`IfcRoof` / `IfcWall` / `IfcSlab` / `IfcCovering`); you still have to pass it.

| Value                                         | Reads                                                                                                                            |
|-----------------------------------------------|----------------------------------------------------------------------------------------------------------------------------------|
| `Const(value)`                                | a fixed string, the same for every object                                                                                        |
| `Attr(key, catalog=…, decimals=…, default=…)` | a building field: `id`, `gml_name`, `lod`, `function`, `measured_height`, `storeys_above_ground`, `roof_type`, or `extra:<name>` |
| `Quantity(key, decimals=…)`                   | geometry of the surface: `area`, `perimeter`, `tilt`, `surface_type`                                                             |

`scope="building"` evaluates once per building and attaches to the `IfcBuilding`;
`scope="surface"` evaluates per surface product and is the only scope where `Quantity` resolves.
In mesh mode both scopes land on the proxy, and `area` / `perimeter` are the building-wide sums.

`catalog` maps a code to a label, either `"funktion"` / `"dachform"` for the built-in
Objektartenkatalog or your own `{code: label}` dict. `decimals` fixes the number of decimal
places; omitting it keeps the source precision. `default` fills in when the attribute is missing —
omit it and the property is dropped for that object instead.

`hamburg_defaults()` is a Python helper for the Hamburg convention; you still have to pass it.
Because a spec is plain data it also serialises, so a pset convention can live in version control:

```python
from bimfabrikhh_core_rs.psets import hamburg_defaults, specs_from_json, specs_to_json

Path("convention.json").write_text(specs_to_json(hamburg_defaults()))
specs = specs_from_json(Path("convention.json").read_text())
```

Only the quantities a spec actually asks for get computed, so dropping `perimeter` and `tilt`
makes the conversion measurably faster.

Properties are written in the order you declare them, both for specs and for the `psets` dicts
handed to `write_ifc`. Combined with deterministic GUIDs this makes the output
byte-reproducible: converting the same input twice yields identical files, so IFC output can be
diffed and checksummed in CI.

---

## Colours

Defaults live in one Rust table (`src/color.rs`) and match BIMFabrikHH: roofs `(1, 0, 0)`,
everything else `(1, 1, 0.498)` including mesh `"Building"`. Pass `colors=` to override; omit
it and those defaults are used. `styles=False` writes no colour at all.

```python
cityjson_to_ifc(page, out, epsg=25832, colors=(0.2, 0.4, 0.8))  # one RGB for every product
cityjson_to_ifc(page, out, epsg=25832, colors={"RoofSurface": (0.6, 0.1, 0.1)})  # rest keep defaults
```

---

## Owner history

`IfcPerson` / `IfcOrganization` / `IfcApplication` default to Salem / bimfabrikhh_core_rs / 0.1.0.
Pass `owner=` to overlay any of those fields; omit it and the defaults stay. `owner_history=False`
writes no owner-history entities at all.

```python
from bimfabrikhh_core_rs.owner import OwnerIdentity

cityjson_to_ifc(page, out, epsg=25832, owner={
    "family_name": "Salem",
    "given_name": "Ahmed",
    "organization": "BIM.Hamburg",
    "application": "bimfabrikhh_core_rs",
    "version": "0.1.0",
})
cityjson_to_ifc(page, out, epsg=25832, owner=OwnerIdentity(organization="BIM.Hamburg"))
```

A plain dict works without pydantic. Only keys you set overlay the defaults.

---

## Examples

Each script in `examples/` is runnable on its own.

| Script                                      | What it does                                                             |
|---------------------------------------------|--------------------------------------------------------------------------|
| `example_one_building_entities.py`          | one CityGML building, typed                                              |
| `example_one_building_block.py`             | one CityGML building, mesh                                               |
| `example_kachel_entities.py` / `_blocks.py` | every building in each tile under `fixtures/`                            |
| `example_generic_city_model.py`             | Hamburg LoD1 tiles, mesh city model                                      |
| `example_hamburg_lod2_cityjson.py`          | Hamburg LoD2 via the CityJSON API, typed                                 |
| `example_hamburg_lod2_cityjson_blocks.py`   | same crop, mesh blocks                                                   |
| `example_pset_spec.py`                      | declarative psets: none unless passed, Hamburg helper, custom convention |

```bash
python examples/example_one_building_entities.py
```

The two Hamburg CityJSON scripts download from the
[Hamburg OGC API](https://api.hamburg.de/datasets/v1/lod2_hamburg) using `urllib`, cache each page
into `output/`, and reuse the cache on later runs. HTTP lives in the examples, never in the
package. `example_generic_city_model.py` expects the LoD1 tiles from `BIMFabrikHH_core` next to
this repository.

The CityGML examples read tiles from `fixtures/`, which are **not** part of the repository because
they run to tens of megabytes each. See [`fixtures/README.md`](fixtures/README.md) for what to put
there. The CityJSON examples need no fixtures.

---

## Layout

```
src/
  lib.rs            module declarations + the pyo3 entry point
  api.rs            pure-Rust pipeline (parse -> export -> write), no pyo3
  bindings/         the pyo3 surface and Python <-> Rust marshalling
  parse.rs          streaming CityGML parser
  cityjson.rs       CityJSON 1.1 parser
  geom.rs           areas, perimeters, tilt, typed vs mesh conversion
  color.rs          default RGB per surface, optional caller overrides
  psetspec.rs       caller-supplied pset rules (empty = none)
  katalog.rs        Objektartenkatalog lookups
  ifc/              GUID, STEP text, IFC4 writer
python/bimfabrikhh_core_rs/
  __init__.py       the importable Python package
  bimfabrikhh_core_rs.*.so   compiled Rust extension (not imported by users)
  psets.py          pydantic pset spec builders (optional extra)
  ifc_types.py      pydantic IFC type map (optional extra)
  owner.py          pydantic IfcOwnerHistory identity (optional extra)
```

Because `api.rs` carries no pyo3 types, the conversion can be driven and tested from plain Rust:

```bash
cargo test
cargo clippy --all-targets
```
