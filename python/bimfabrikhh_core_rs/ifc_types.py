"""Map CityGML / CityJSON surface types to IFC entity names.

Same pattern as ``bimfabrikhh_core_rs.psets``: you pass the map in, Rust applies it,
and omitting ``ifc_types`` means no special typing — every product is
``IfcBuildingElementProxy``. There is no CityGML → IFC table in Rust.

    from bimfabrikhh_core_rs import cityjson_to_ifc
    from bimfabrikhh_core_rs.ifc_types import IfcTypeMap, hamburg_ifc_types

    cityjson_to_ifc(page, out, epsg=25832, ifc_types=hamburg_ifc_types())

    cityjson_to_ifc(
        page, out, epsg=25832,
        ifc_types=IfcTypeMap(RoofSurface="IfcRoof", WallSurface="IfcWall"),
    )

A plain dict works without pydantic. Mesh mode reads the ``Building`` key.
"""

from __future__ import annotations

from typing import Dict

try:
    from pydantic import BaseModel, ConfigDict, Field, field_validator, model_validator
except ModuleNotFoundError as exc:  # pragma: no cover
    raise ModuleNotFoundError(
            "bimfabrikhh_core_rs.ifc_types needs pydantic. Install it with "
            '`pip install "bimfabrikhh_core_rs[psets]"` or `pip install pydantic`.'
    ) from exc

__all__ = [
        "IfcTypeMap",
        "SURFACE_KEYS",
        "hamburg_ifc_types",
]

SURFACE_KEYS = (
        "RoofSurface",
        "WallSurface",
        "GroundSurface",
        "OuterFloorSurface",
        "OuterCeilingSurface",
        "Building",
)


class IfcTypeMap(BaseModel):
    """Caller-supplied surface type → IFC entity map.

    Construct from keyword arguments or a dict::

        IfcTypeMap(RoofSurface="IfcRoof", WallSurface="IfcWall")
        IfcTypeMap({"RoofSurface": "IfcRoof"})
    """

    model_config = ConfigDict(extra="forbid")

    types: Dict[str, str] = Field(min_length=1)

    def __init__(self, types=None, **kwargs) -> None:
        if types is not None and "types" not in kwargs:
            kwargs["types"] = types
        super().__init__(**kwargs)

    @model_validator(mode="before")
    @classmethod
    def _accept_flat_dict(cls, data):
        if isinstance(data, dict) and "types" not in data:
            return {"types": data}
        return data

    @field_validator("types")
    @classmethod
    def _ifc_names(cls, v: Dict[str, str]) -> Dict[str, str]:
        for key, entity in v.items():
            if not key.strip():
                raise ValueError("surface type keys must be non-empty")
            if not entity.strip():
                raise ValueError(f"{key!r}: IFC entity name must be non-empty")
            if not entity.lower().startswith("ifc"):
                raise ValueError(f"{key!r}: {entity!r} must start with 'Ifc' (e.g. IfcWall)")
        return v

    def to_wire(self) -> dict:
        return dict(self.types)


def hamburg_ifc_types() -> IfcTypeMap:
    """The usual CityGML LoD2 surface map. Pass it explicitly; Rust has no copy."""
    return IfcTypeMap(
            RoofSurface="IfcRoof",
            WallSurface="IfcWall",
            GroundSurface="IfcSlab",
            OuterFloorSurface="IfcSlab",
            OuterCeilingSurface="IfcCovering",
            Building="IfcBuildingElementProxy",
    )
