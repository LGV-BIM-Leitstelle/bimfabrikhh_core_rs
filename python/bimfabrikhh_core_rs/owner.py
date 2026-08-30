"""FILE_NAME + IfcOwnerHistory identity.

Same pattern as ``bimfabrikhh_core_rs.psets``: edit the defaults here, pass
``owner=defaults()`` into the writer. Rust writes the header and one shared
``IfcOwnerHistory``. ``owner_history=False`` skips the data-section block;
``FILE_NAME`` is still filled from this config.

    from bimfabrikhh_core_rs import trees_to_ifc
    from bimfabrikhh_core_rs.owner import defaults

    trees_to_ifc(trees, out, owner=defaults())

A plain dict works without pydantic. Only keys you set overlay the Rust default.
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
    "defaults",
]


class OwnerIdentity(BaseModel):
    """Caller-supplied FILE_NAME / IfcPerson / IfcOrganization / IfcApplication.

    Unset fields keep the values below. ``to_wire()`` drops empties so a
    partial dict still overlays cleanly::

        OwnerIdentity(organization="BIM.Hamburg", given_name="Ahmed")
    """

    model_config = ConfigDict(extra="forbid")

    # FILE_NAME author ← given_name + family_name; also IfcPerson
    family_name: Optional[str] = Field(default="Salem")
    given_name: Optional[str] = Field(default="Ahmed")
    # FILE_NAME organization; also IfcOrganization
    organization: Optional[str] = Field(default="BIM-Leitstelle, LGV Hamburg")
    # FILE_NAME preprocessor_version (toolbox, not the product name)
    preprocessor: Optional[str] = Field(default="bimfabrikhh_core_rs 0.1.1")
    # FILE_NAME originating_system (the application)
    originating_system: Optional[str] = Field(default="BIMFabrikHH")
    # FILE_NAME authorization
    authorization: Optional[str] = Field(default="")
    # IfcApplication ApplicationFullName / Version / ApplicationIdentifier
    application: Optional[str] = Field(default="BIMFabrikHH")
    version: Optional[str] = Field(default="0.1.1")
    identifier: Optional[str] = Field(default="BIMFabrikHH")

    def to_wire(self) -> dict:
        return {k: v for k, v in self.model_dump().items() if v}


def defaults() -> OwnerIdentity:
    """Hamburg FILE_NAME + IfcApplication values from this module."""
    return OwnerIdentity()
