"""Pydantic attribute mapping. Missing source fields are not written to IFC."""

from __future__ import annotations

from typing import List

from bimfabrikhh_core_rs.psets import Attr, Const, PsetSpec, bimfabrik_quantities, hamburg_hyperlink


def objektinformation(
    *,
    id_ebene: str = "Stadtmodell",
    loi: str = "300",
    bemerkung: str = "undefiniert",
) -> PsetSpec:
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
            # CityGML / CityJSON building (typical LoD1–2)
            "_FunktionGebaeude": Attr("function", catalog="funktion"),
            "_RelativeHoehe": Attr("measured_height"),
            "_AnzahlObergeschoss": Attr("storeys_above_ground"),
            "_Dachform": Attr("roof_type", catalog="dachform"),
            "_Grundrissaktualitaet": Attr("extra:Grundrissaktualitaet"),
            # Hamburg LoD3 CityGRID generics
            "_CitygridUnitID": Attr("extra:citygrid_UnitID"),
            "_Grundflaeche2D": Attr("extra:Grundfläche 2D"),
            "_GrundhoeheNN": Attr("extra:Grundhöhe NN"),
        },
    )


def specs(*, hyperlink: bool = True, quantities: bool = False) -> List[PsetSpec]:
    out = [objektinformation()]
    if hyperlink:
        out.append(hamburg_hyperlink())
    if quantities:
        out.append(bimfabrik_quantities())
    return out
