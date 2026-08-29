//! CityGML / CityJSON → IFC4 STEP, callable from Python.
//!
//! Crate layout:
//! - [`api`] — pure-Rust pipeline (parse → export → write), no pyo3
//! - [`bindings`] — the pyo3 surface, thin wrappers over [`api`]
//! - [`parse`] — streaming CityGML 1.0 / 2.0 building parser
//! - [`cityjson`] — CityJSON 1.1 (file or text, no HTTP)
//! - [`katalog`] — Hamburg / Sachsen Objektartenkatalog lookups
//! - [`psetspec`] — caller-supplied pset rules, evaluated per building and
//!   surface; empty list means no property sets
//! - [`color`] — default RGB per surface, optional caller overrides
//! - [`geom`] — area / perimeter / tilt and typed vs mesh conversion
//! - [`ifc`] — GUID, STEP text, IFC4 writer
//! - [`types`] — shared parse / export structs

mod api;
mod cityjson;
mod color;
mod geom;
mod ifc;
mod katalog;
mod parse;
mod psetspec;
mod bindings;
mod types;

use pyo3::prelude::*;
use pyo3::types::PyModule;

#[pymodule]
fn bimfabrikhh_core_rs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    bindings::register(m)
}
