# fixtures

CityGML test tiles live here. **The `.gml` files are not in the repository** — they are tens of
megabytes each and are ignored by `.gitignore`. Drop them in this directory yourself.

## What the examples expect

| Example                                  | Needs                                    |
|------------------------------------------|------------------------------------------|
| `example_one_building_entities.py`       | `lod2_33410_5652_2_sn.gml`               |
| `example_one_building_block.py`          | `lod2_33410_5652_2_sn.gml`               |
| `example_one_building_block_layer.py`    | `lod2_33410_5652_2_sn.gml`               |
| `example_kachel_entities.py`             | any `*.gml` in this directory            |
| `example_kachel_blocks.py`               | any `*.gml` in this directory            |
| `example_hamburg_lod3.py` / `_blocks.py` / `_bbox*.py` | none here — set `BIMFABRIKHH_LOD3_GML_DIR` in `.env` |

`lod2_33410_5652_2_sn.gml` is a Sachsen LoD2 tile (EPSG:25833), which is why the CityGML entry
points default to `epsg=25833`. Sachsen publishes LoD2 tiles as open data through the
[Geoportal Sachsen](https://www.geodaten.sachsen.de/). Any CityGML 1.0 or 2.0 building tile works;
the tile name only matters for the two `example_one_building_*` scripts.

The CityJSON examples need nothing here: they download Hamburg LoD2 in memory and write only IFC.

## Checking a tile before converting

```python
from bimfabrikhh_core_rs import parse_gml

buildings = parse_gml("fixtures/your_tile.gml")
print(len(buildings), buildings[0]["id"], len(buildings[0]["boundaries"]))
```
