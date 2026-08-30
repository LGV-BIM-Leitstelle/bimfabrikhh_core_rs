"""Declarative property-set specifications.

A spec describes *rules*, not values. It is validated once here and then
evaluated inside Rust for every building and every surface, so the conversion
hot path never touches a Python object. That keeps full control over property
names, defaults, number formatting and catalogue mappings without paying for it
per building.

    from bimfabrikhh_core_rs import cityjson_to_ifc
    from bimfabrikhh_core_rs.psets import Attr, Const, PsetSpec, Quantity

    objektinformation = PsetSpec(
        name="Pset_Objektinformation",
        scope="building",
        properties={
            "_IDEbene1":       Const("Stadtmodell"),
            "_StadtmodellLoD": Attr("lod"),
            "_Dachform":       Attr("roof_type", catalog="dachform"),
        },
    )
    cityjson_to_ifc(page, out, epsg=25832, psets=[objektinformation])

Omit ``psets`` and the IFC is written without property sets. There is no
Rust-side fallback. ``hamburg_defaults()`` is a Python helper if you want the
Hamburg convention.

Requires pydantic (``pip install 'bimfabrikhh_core_rs[psets]'``). Plain dicts in the same
wire shape work without it.
"""

from __future__ import annotations

import json
from typing import Annotated, Dict, List, Literal, Optional, Sequence, Union

try:
    from pydantic import BaseModel, ConfigDict, Field, field_validator, model_validator
except ModuleNotFoundError as exc:  # pragma: no cover
    raise ModuleNotFoundError(
        'bimfabrikhh_core_rs.psets needs pydantic. Install it with `pip install "bimfabrikhh_core_rs[psets]"` or `pip install pydantic`.'
    ) from exc

__all__ = [
    "Attr",
    "Const",
    "PsetSpec",
    "Quantity",
    "ATTR_KEYS",
    "TREE_ATTR_KEYS",
    "QUANTITY_KEYS",
    "bimfabrik_quantities",
    "hamburg_defaults",
    "hamburg_hyperlink",
    "hamburg_objektinformation",
    "specs_from_json",
    "specs_to_json",
]

#: Building fields an :class:`Attr` may read. ``extra:<name>`` reaches any
#: attribute the parser kept but does not model explicitly.
ATTR_KEYS = (
    "id",
    "gml_name",
    "lod",
    "function",
    "measured_height",
    "storeys_above_ground",
    "roof_type",
)

#: Tree record / mesh fields an :class:`Attr` may read (``trees_to_ifc``).
TREE_ATTR_KEYS = (
    "name",
    "trunk_radius",
    "trunk_height",
    "crown_radius",
    "art_baum",
    "aufnahmedatum_vermessung",
    "baumhoehe",
    "baumhoehe_bemerkung",
    "baumnummer",
    "bemerkung",
    "bezirk",
    "gattung_deutsch",
    "kronendurchmesser",
    "pflanzjahr",
    "stadtteil",
    "stammdurchmesser",
    "status_vegetation",
    "strassenname",
)

#: Geometric quantities a :class:`Quantity` may read. In mesh mode ``area`` and
#: ``perimeter`` are the building-wide sums.
QUANTITY_KEYS = ("area", "perimeter", "tilt", "surface_type")


class Const(BaseModel):
    """A fixed value, identical for every object."""

    model_config = ConfigDict(extra="forbid")

    kind: Literal["const"] = "const"
    value: str

    def __init__(self, value: Optional[str] = None, **kw) -> None:
        if value is not None:
            kw["value"] = value
        super().__init__(**kw)


class Attr(BaseModel):
    """A value read from the parsed building.

    ``catalog`` maps a code to a label, either by name (``"funktion"`` or
    ``"dachform"``, using the built-in Objektartenkatalog) or with your own
    ``{code: label}`` dict. ``decimals`` fixes the number of decimal places for
    numeric fields; leaving it out keeps the source precision. ``default``
    applies when the attribute is missing or the code is not in the catalogue --
    ``None`` drops the property instead.
    """

    model_config = ConfigDict(extra="forbid")

    kind: Literal["attr"] = "attr"
    key: str
    catalog: Optional[Union[Literal["funktion", "dachform"], Dict[str, str]]] = None
    decimals: Optional[int] = Field(default=None, ge=0, le=15)
    default: Optional[str] = None

    def __init__(self, key: Optional[str] = None, **kw) -> None:
        if key is not None:
            kw["key"] = key
        super().__init__(**kw)

    @field_validator("key")
    @classmethod
    def _known_key(cls, v: str) -> str:
        if v.startswith("extra:"):
            if not v[len("extra:") :]:
                raise ValueError("'extra:' needs an attribute name after the colon")
            return v
        if v in ATTR_KEYS or v in TREE_ATTR_KEYS:
            return v
        raise ValueError(
            f"unknown attribute key {v!r}; expected one of {', '.join(ATTR_KEYS)}, "
            f"{', '.join(TREE_ATTR_KEYS)} or 'extra:<name>'"
        )


class Quantity(BaseModel):
    """A geometric quantity of the surface, computed in Rust."""

    model_config = ConfigDict(extra="forbid")

    kind: Literal["quantity"] = "quantity"
    key: str
    decimals: Optional[int] = Field(default=None, ge=0, le=15)

    def __init__(self, key: Optional[str] = None, **kw) -> None:
        if key is not None:
            kw["key"] = key
        super().__init__(**kw)

    @field_validator("key")
    @classmethod
    def _known_key(cls, v: str) -> str:
        if v not in QUANTITY_KEYS:
            raise ValueError(f"unknown quantity key {v!r}; expected one of {', '.join(QUANTITY_KEYS)}")
        return v


PropertyValue = Annotated[Union[Const, Attr, Quantity], Field(discriminator="kind")]


class PsetSpec(BaseModel):
    """One IFC property set and the rules that fill it.

    ``scope="building"`` attaches the set to the ``IfcBuilding`` and evaluates it
    once per building. ``scope="surface"`` attaches it to each surface product
    and is the only scope where :class:`Quantity` resolves.
    """

    model_config = ConfigDict(extra="forbid")

    name: str
    scope: Literal["building", "surface"] = "building"
    properties: Dict[str, PropertyValue] = Field(min_length=1)

    @model_validator(mode="after")
    def _quantities_need_surface_scope(self) -> "PsetSpec":
        if self.scope == "surface":
            return self
        offenders = [n for n, v in self.properties.items() if isinstance(v, Quantity)]
        if offenders:
            raise ValueError(
                f"pset {self.name!r} has scope 'building' but reads geometry in "
                f"{', '.join(offenders)}; quantities only resolve with scope='surface'"
            )
        return self

    def to_wire(self) -> dict:
        """The plain-data form Rust consumes. Order of properties is preserved."""
        return {
            "name": self.name,
            "scope": self.scope,
            "properties": [[k, val.model_dump()] for k, val in self.properties.items()],
        }


def hamburg_objektinformation(
    *,
    id_ebene: str = "Stadtmodell",
    loi: str = "300",
    bemerkung: str = "undefiniert",
) -> PsetSpec:
    """``Pset_Objektinformation`` as the built-in Hamburg export writes it.

    Source-backed ``Attr`` keys have no ``default``: if the GML/CityJSON field
    is missing, that property is omitted from the IFC.
    """
    return PsetSpec(
        name="Pset_Objektinformation",
        scope="building",
        properties={
            "_IDEbene1": Const(id_ebene),
            "_IDEbene2": Const(id_ebene),
            "_IDEbene3": Const(id_ebene),
            "_LoI": Const(loi),
            "_Bemerkung": Const(bemerkung),
            "_StadtmodellLoD": Attr("lod"),
            "_FunktionGebaeude": Attr("function", catalog="funktion"),
            "_RelativeHoehe": Attr("measured_height"),
            "_AnzahlObergeschoss": Attr("storeys_above_ground"),
            "_Dachform": Attr("roof_type", catalog="dachform"),
        },
    )


def hamburg_hyperlink(
    url: str = "www.bim.hamburg.de",
    remark: str = "Link zur Homepage von BIM.Hamburg",
) -> PsetSpec:
    """``Pset_Hyperlink`` as the built-in Hamburg export writes it."""
    return PsetSpec(
        name="Pset_Hyperlink",
        scope="building",
        properties={
            "_Hyperlink_001": Const(url),
            "_Hyperlink_001_Bemerkung": Const(remark),
        },
    )


def bimfabrik_quantities() -> PsetSpec:
    """``BIMFabrikHH_Quantities`` as the built-in export writes it."""
    return PsetSpec(
        name="BIMFabrikHH_Quantities",
        scope="surface",
        properties={
            "GrossArea": Quantity("area", decimals=4),
            "Perimeter": Quantity("perimeter", decimals=4),
            "Tilt": Quantity("tilt", decimals=2),
            "SurfaceType": Quantity("surface_type"),
        },
    )


def hamburg_defaults(*, hyperlink: bool = True, quantities: bool = False) -> List[PsetSpec]:
    """Hamburg property sets as specs you pass explicitly. Rust has no fallback."""
    specs = [hamburg_objektinformation()]
    if hyperlink:
        specs.append(hamburg_hyperlink())
    if quantities:
        specs.append(bimfabrik_quantities())
    return specs


def specs_to_json(specs: Sequence[PsetSpec], *, indent: int = 2) -> str:
    """Serialise specs so a pset convention can live in version control."""
    return json.dumps([s.model_dump() for s in specs], indent=indent, ensure_ascii=False)


def specs_from_json(text: str) -> List[PsetSpec]:
    """Load specs written by :func:`specs_to_json`, validating as they load."""
    return [PsetSpec.model_validate(item) for item in json.loads(text)]
