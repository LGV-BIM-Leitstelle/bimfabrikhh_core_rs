"""Hamburg LoD3 CityGRID tiles. Path only — conversion stays in the examples."""

from pathlib import Path

from hamburg_lod2 import BBOX_EPSG, BBOX_WGS84

LOD3 = Path(
    "/mnt/c/_Lokale_Daten_ungesichert/___BIMFabrikHH_Datensaetze/LoD3-HH_Area1_2023_12_14/LoD3-GML"
)
# Set True to convert every *.gml in LOD3.
LOOP = True
TILE = "6627.gml"

# Same crop as the LoD2 CityJSON examples.
__all__ = ["LOD3", "LOOP", "TILE", "BBOX_EPSG", "BBOX_WGS84"]
