"""Hamburg LoD3 CityGRID tiles. Folder comes from ``.env``, not this file."""

from envpaths import lod3_dir
from hamburg_lod2 import BBOX_EPSG, BBOX_WGS84

LOD3 = lod3_dir()
# Full Stadtmodell_LoD3 is large — set True to convert every *.gml.
LOOP = False
TILE = "6627.gml"

# Same crop as the LoD2 CityJSON examples.
__all__ = ["LOD3", "LOOP", "TILE", "BBOX_EPSG", "BBOX_WGS84"]
