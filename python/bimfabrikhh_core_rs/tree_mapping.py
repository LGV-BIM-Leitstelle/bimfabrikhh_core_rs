"""Hamburg tree property sets as specs, same idea as ``attribute_mapping``.

Rules, not values. Pass ``specs()`` to ``trees_to_ifc(..., psets=specs())``.
Rust fills each tree from ``attributes`` plus derived crown/trunk diameters.
Missing ``Attr`` fields are omitted. Constants match BIMFabrik ``Pset_Objektinformation_Tree``
/ ``Pset_Bauwerk_Tree``.
"""

from __future__ import annotations

from typing import List

from bimfabrikhh_core_rs.psets import Attr, Const, PsetSpec


def objektinformation() -> PsetSpec:
    return PsetSpec(
        name="Pset_Objektinformation",
        scope="building",
        properties={
            "_IDEbene1": Const("Gehoelz"),
            "_IDEbene2": Const("Baum"),
            "_IDEbene3": Const("Baum"),
            "_LoG": Const("100"),
            "_LoI": Const("100"),
            "_ArtBaum": Attr("art_baum"),
            "_AufnahmedatumVermessung": Attr("aufnahmedatum_vermessung"),
            "_Baumhoehe": Attr("baumhoehe", decimals=2),
            "_BaumhoeheBemerkung": Attr("baumhoehe_bemerkung"),
            "_Baumnummer": Attr("baumnummer"),
            "_Bemerkung": Attr("bemerkung"),
            "_Bezirk": Attr("bezirk"),
            "_Gattung": Attr("gattung_deutsch"),
            "_Kronendurchmesser": Attr("kronendurchmesser", decimals=2),
            "_Pflanzjahr": Attr("pflanzjahr"),
            "_Stadtteil": Attr("stadtteil"),
            "_Stammdurchmesser": Attr("stammdurchmesser", decimals=2),
            "_StatusVegetation": Attr("status_vegetation"),
        },
    )


def bauwerk() -> PsetSpec:
    return PsetSpec(
        name="Pset_Bauwerk",
        scope="building",
        properties={
            "_Strassenname": Attr("strassenname"),
        },
    )


def specs() -> List[PsetSpec]:
    return [objektinformation(), bauwerk()]
