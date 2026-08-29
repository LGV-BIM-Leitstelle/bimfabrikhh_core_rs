"""Identity written into IfcPerson / IfcOrganization / IfcApplication.

Same pattern as ``bimfabrikhh_core_rs.ifc_types``: you pass the identity in, Rust writes
it. Omit ``owner=`` and the Rust default is used (Salem / bimfabrikhh_core_rs / 0.1.0).
``owner_history=False`` skips the whole owner-history block.

    from bimfabrikhh_core_rs import cityjson_to_ifc
    from bimfabrikhh_core_rs.owner import OwnerIdentity

    cityjson_to_ifc(
        page, out, epsg=25832,
        owner=OwnerIdentity(
            family_name="Salem",
            given_name="Ahmed",
            organization="BIM.Hamburg",
            application="bimfabrikhh_core_rs",
            version="0.1.0",
        ),
    )

A plain dict works without pydantic. Only keys you set overlay the defaults.
"""

from __future__ import annotations

from typing import Optional

try:
    from pydantic import BaseModel, ConfigDict, Field
except ModuleNotFoundError as exc:  # pragma: no cover
    raise ModuleNotFoundError(
        'bimfabrikhh_core_rs.owner needs pydantic. Install it with `pip install "bimfabrikhh_core_rs[psets]"` or `pip install pydantic`.'
    ) from exc

__all__ = [
    "OwnerIdentity",
]


class OwnerIdentity(BaseModel):
    """Caller-supplied IfcOwnerHistory identity.

    Unset fields keep the Rust default. ``to_wire()`` drops empties so a
    partial dict still overlays cleanly::

        OwnerIdentity(organization="BIM.Hamburg", given_name="Ahmed")
    """

    model_config = ConfigDict(extra="forbid")

    family_name: Optional[str] = Field(default=None)
    given_name: Optional[str] = Field(default=None)
    organization: Optional[str] = Field(default=None)
    application: Optional[str] = Field(default=None)
    version: Optional[str] = Field(default=None)
    identifier: Optional[str] = Field(default=None)

    def to_wire(self) -> dict:
        return {k: v for k, v in self.model_dump().items() if v}
