"""``IfcProjectedCRS`` values for the ``crs=`` argument of the writers.

Rust has no PROJ, so the caller names the map CRS of the coordinates. Omit
``crs`` and the IFC gets no georeferencing. Coordinates are never reprojected:
transform them before writing (BIMFabrikHH_core does this with pyproj and
passes its ``CoordinateSystem``, which works here as well).

    from bimfabrikhh_core_rs import trees_to_ifc
    from bimfabrikhh_core_rs.crs import etrs89_utm

    trees_to_ifc(trees, out, crs=etrs89_utm(32, vertical_datum="EPSG:7837"))

Plain dicts, no pydantic needed.
"""

from __future__ import annotations

from typing import Dict, Optional

__all__ = [
    "etrs89_utm",
    "projected_crs",
]


def projected_crs(
    name: str,
    geodetic_datum: str,
    *,
    description: Optional[str] = None,
    vertical_datum: Optional[str] = None,
    map_projection: Optional[str] = None,
    map_zone: Optional[str] = None,
) -> Dict[str, Optional[str]]:
    """``IfcProjectedCRS`` fields; ``None`` fields are written as unset (``$``).

    ``name`` is usually ``"EPSG:<code>"``. ``vertical_datum`` names the height
    system of the Z values, e.g. ``"EPSG:7837"`` (DHHN2016); leave it ``None``
    when it is unknown.
    """
    return {
        "name": name,
        "description": description,
        "geodetic_datum": geodetic_datum,
        "vertical_datum": vertical_datum,
        "map_projection": map_projection,
        "map_zone": map_zone,
    }


def etrs89_utm(zone: int, *, vertical_datum: Optional[str] = None) -> Dict[str, Optional[str]]:
    """ETRS89 / UTM zone ``zone``N (EPSG:258<zone>, zones 28-38), e.g. 32 for Hamburg."""
    if not 28 <= zone <= 38:
        raise ValueError(f"ETRS89 / UTM zone must be 28-38, got {zone}")
    return projected_crs(
        f"EPSG:{25800 + zone}",
        "ETRS89",
        description=f"ETRS89 / UTM zone {zone}N",
        vertical_datum=vertical_datum,
        map_projection="Transverse Mercator",
        map_zone=str(zone),
    )
