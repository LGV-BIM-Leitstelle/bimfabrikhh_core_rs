"""Hamburg DGM property sets as specs, same idea as ``tree_mapping``.

Rules, not values. Pass ``specs()`` to ``terrain_to_ifc(..., psets=specs())``.
Constants match BIMFabrik ``Pset_Objektinformation_DGM`` plus ``Pset_Hyperlink``.
"""

from __future__ import annotations

from typing import List

from bimfabrikhh_core_rs.psets import Const, PsetSpec, hamburg_hyperlink


def objektinformation() -> PsetSpec:
    return PsetSpec(
        name="Pset_Objektinformation",
        scope="building",
        properties={
            "_ArtDGM": Const("Netz"),
            "_AufnahmedatumHinweis": Const("undefiniert"),
            "_AufnahmedatumVermessung": Const("undefiniert"),
            "_Bauphase": Const("Vorarbeiten"),
            "_Bemerkung": Const("undefiniert"),
            "_DatenHerkunft": Const("SDP"),
            "_IDEbene1": Const("Gelaende"),
            "_IDEbene2": Const("Erdoberflaeche"),
            "_IDEbene3": Const("Erdoberflaeche"),
            "_LoG": Const("300", ifc_type="IfcInteger"),
            "_LoI": Const("100", ifc_type="IfcInteger"),
        },
    )


def specs(*, hyperlink: bool = True) -> List[PsetSpec]:
    out = [objektinformation()]
    if hyperlink:
        out.append(hamburg_hyperlink())
    return out
