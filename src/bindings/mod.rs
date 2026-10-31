//! The pyo3 surface. Every function here is a thin wrapper over [`crate::api`].

mod convert;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyList, PyModule, PyTuple};

use crate::api::{self, ExportOptions};
use crate::cityjson::{parse_cityjson_file, parse_cityjson_json, parse_cityjson_str};
use crate::parse::parse_gml_file;
use crate::srs::{cityjson_declared_crs, gml_declared_crs};
use crate::types::ParsedBuilding;
use convert::{
    basepoint_psets_from_py, basepoint_size_from_py, basepoint_xyz, building_to_py, cityjson_source_from_py, colors_from_py, crs_from_py, gml_paths_from_py,
    ifc_types_from_py, owner_from_py, pset_specs_from_py, products_from_py, terrain_from_py,
    terrain_parts_from_py, trees_from_py, xy_transform_from_py, xyz_list_from_py, faces_from_py,
};

type Bbox = Option<(f64, f64, f64, f64)>;

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(parse_gml, m)?)?;
    m.add_function(wrap_pyfunction!(write_ifc, m)?)?;
    m.add_function(wrap_pyfunction!(gml_to_ifc, m)?)?;
    m.add_function(wrap_pyfunction!(parse_cityjson, m)?)?;
    m.add_function(wrap_pyfunction!(parse_cityjson_text, m)?)?;
    m.add_function(wrap_pyfunction!(cityjson_to_ifc, m)?)?;
    m.add_function(wrap_pyfunction!(trees_to_ifc, m)?)?;
    m.add_function(wrap_pyfunction!(terrain_to_ifc, m)?)?;
    m.add_function(wrap_pyfunction!(terrain_parts_to_ifc, m)?)?;
    m.add_function(wrap_pyfunction!(declared_crs, m)?)?;
    Ok(())
}

fn buildings_to_py(parsed: Result<Vec<ParsedBuilding>, String>) -> PyResult<Vec<PyObject>> {
    let buildings = parsed.map_err(PyValueError::new_err)?;
    Python::with_gil(|py| buildings.iter().map(|b| building_to_py(py, b)).collect())
}

#[allow(clippy::too_many_arguments)]
fn export_options(
    mode: &str,
    project_name: &str,
    site_name: &str,
    crs: Option<&Bound<'_, PyAny>>,
    styles: bool,
    cad_layer: bool,
    owner_history: bool,
    basepoint: Option<Bound<'_, PyTuple>>,
    psets: Option<&Bound<'_, PyAny>>,
    ifc_types: Option<&Bound<'_, PyAny>>,
    colors: Option<&Bound<'_, PyAny>>,
    owner: Option<&Bound<'_, PyAny>>,
    progress: bool,
    basepoint_psets: Option<&Bound<'_, PyAny>>,
    basepoint_size: Option<f64>,
) -> PyResult<ExportOptions> {
    Ok(ExportOptions {
        mode: mode.into(),
        project_name: project_name.into(),
        site_name: site_name.into(),
        crs: crs_from_py(crs)?,
        styles,
        cad_layer,
        owner_history,
        owner: owner_from_py(owner)?,
        basepoint: basepoint_xyz(basepoint)?,
        basepoint_size: basepoint_size_from_py(basepoint_size)?,
        basepoint_psets: basepoint_psets_from_py(basepoint_psets)?,
        psets: pset_specs_from_py(psets)?,
        ifc_types: ifc_types_from_py(ifc_types)?,
        colors: colors_from_py(colors)?,
        progress,
    })
}

/// CRS a CityGML file (first `srsName`) or CityJSON file (`.json`,
/// `metadata.referenceSystem`) declares, as
/// ``{"srs_name": str, "horizontal_epsg": int | None, "vertical_epsg": int | None}``,
/// or ``None`` when it declares none. `gml_to_ifc` / `cityjson_to_ifc` use the
/// same check: a contradicting `crs` raises, a missing `vertical_datum` is filled.
#[pyfunction]
fn declared_crs(py: Python<'_>, path: &str) -> PyResult<Option<PyObject>> {
    let declared = if path.to_ascii_lowercase().ends_with(".json") {
        let text = std::fs::read_to_string(path).map_err(|e| PyValueError::new_err(format!("open {path}: {e}")))?;
        cityjson_declared_crs(&parse_cityjson_json(&text).map_err(PyValueError::new_err)?)
    } else {
        gml_declared_crs(path).map_err(PyValueError::new_err)?
    };
    declared
        .map(|crs| {
            let d = PyDict::new(py);
            d.set_item("srs_name", crs.srs_name)?;
            d.set_item("horizontal_epsg", crs.horizontal_epsg)?;
            d.set_item("vertical_epsg", crs.vertical_epsg)?;
            Ok(d.into_any().unbind())
        })
        .transpose()
}

/// Parse CityGML into building dicts.
#[pyfunction]
#[pyo3(signature = (path, building_id_filter=None, bbox_epsg=None, progress=false))]
fn parse_gml(
    path: &str,
    building_id_filter: Option<&str>,
    bbox_epsg: Bbox,
    progress: bool,
) -> PyResult<Vec<PyObject>> {
    buildings_to_py(parse_gml_file(path, building_id_filter, bbox_epsg, progress))
}

/// Parse a CityJSON file into building dicts (same shape as ``parse_gml``).
#[pyfunction]
#[pyo3(signature = (path, building_id_filter=None, bbox_epsg=None))]
fn parse_cityjson(path: &str, building_id_filter: Option<&str>, bbox_epsg: Bbox) -> PyResult<Vec<PyObject>> {
    buildings_to_py(parse_cityjson_file(path, building_id_filter, bbox_epsg))
}

/// Parse CityJSON from a string. HTTP stays outside this package.
#[pyfunction]
#[pyo3(signature = (text, building_id_filter=None, bbox_epsg=None))]
fn parse_cityjson_text(text: &str, building_id_filter: Option<&str>, bbox_epsg: Bbox) -> PyResult<Vec<PyObject>> {
    buildings_to_py(parse_cityjson_str(text, building_id_filter, bbox_epsg))
}

/// Write IFC4 STEP from a list of product dicts.
#[pyfunction]
#[pyo3(signature = (products, path, project_name="bimfabrikhh_core_rs", site_name="Site", crs=None, styles=true, cad_layer=false, owner_history=true, basepoint=None, owner=None, progress=false, basepoint_psets=None, basepoint_size=None))]
#[allow(clippy::too_many_arguments)]
fn write_ifc(
    products: Bound<'_, PyList>,
    path: &str,
    project_name: &str,
    site_name: &str,
    crs: Option<Bound<'_, PyAny>>,
    styles: bool,
    cad_layer: bool,
    owner_history: bool,
    basepoint: Option<Bound<'_, PyTuple>>,
    owner: Option<Bound<'_, PyAny>>,
    progress: bool,
    basepoint_psets: Option<Bound<'_, PyAny>>,
    basepoint_size: Option<f64>,
) -> PyResult<String> {
    let parsed = products_from_py(&products)?;
    let opts = export_options(
        "typed",
        project_name,
        site_name,
        crs.as_ref(),
        styles,
        cad_layer,
        owner_history,
        basepoint,
        None,
        None,
        None,
        owner.as_ref(),
        progress,
        basepoint_psets.as_ref(),
        basepoint_size,
    )?;
    api::products_to_ifc(parsed, path, &opts).map_err(PyValueError::new_err)
}

/// Parse CityGML (one file or many) and write a single IFC. `mode` is `"typed"` or `"mesh"`.
///
/// `psets` takes `bimfabrikhh_core_rs.psets.PsetSpec` objects (or their dicts).
/// Omit it and the IFC is written without property sets.
/// `ifc_types` maps CityGML/CityJSON surface types to IFC entities
/// (`bimfabrikhh_core_rs.ifc_types.IfcTypeMap` or a dict). Omit it and every product is
/// `IfcBuildingElementProxy`.
/// `crs` is the map CRS of the written coordinates (see [`crs_from_py`]); omit
/// it and the IFC gets no `IfcProjectedCRS` / `IfcMapConversion`. It is
/// checked against the `srsName` of the files (see [`declared_crs`]).
/// `bbox_epsg` filters in the CRS of the source file. `transform` is an
/// optional callable `transform(xs, ys) -> (xs, ys)` (e.g. pyproj
/// `Transformer.transform`), called once with all x/y of the kept buildings;
/// Z is not changed. Without it the coordinates are written as parsed.
/// `owner` overlays `IfcPerson` / `IfcOrganization` / `IfcApplication`
/// (`bimfabrikhh_core_rs.owner.OwnerIdentity` or a dict). Omit it and the Rust default
/// is used. `owner_history=false` writes no owner-history block.
#[pyfunction]
#[pyo3(signature = (gml_path, ifc_path, building_id_filter=None, mode="typed", project_name="bimfabrikhh_core_rs", site_name="Site", crs=None, styles=true, cad_layer=false, owner_history=true, bbox_epsg=None, transform=None, basepoint=None, psets=None, ifc_types=None, colors=None, owner=None, progress=false, basepoint_psets=None, basepoint_size=None))]
#[allow(clippy::too_many_arguments)]
fn gml_to_ifc(
    gml_path: Bound<'_, PyAny>,
    ifc_path: &str,
    building_id_filter: Option<&str>,
    mode: &str,
    project_name: &str,
    site_name: &str,
    crs: Option<Bound<'_, PyAny>>,
    styles: bool,
    cad_layer: bool,
    owner_history: bool,
    bbox_epsg: Bbox,
    transform: Option<Bound<'_, PyAny>>,
    basepoint: Option<Bound<'_, PyTuple>>,
    psets: Option<Bound<'_, PyAny>>,
    ifc_types: Option<Bound<'_, PyAny>>,
    colors: Option<Bound<'_, PyAny>>,
    owner: Option<Bound<'_, PyAny>>,
    progress: bool,
    basepoint_psets: Option<Bound<'_, PyAny>>,
    basepoint_size: Option<f64>,
) -> PyResult<String> {
    let paths = gml_paths_from_py(&gml_path)?;
    let opts = export_options(
        mode,
        project_name,
        site_name,
        crs.as_ref(),
        styles,
        cad_layer,
        owner_history,
        basepoint,
        psets.as_ref(),
        ifc_types.as_ref(),
        colors.as_ref(),
        owner.as_ref(),
        progress,
        basepoint_psets.as_ref(),
        basepoint_size,
    )?;
    let mut transform = transform.as_ref().map(xy_transform_from_py);
    api::gml_to_ifc(&paths, ifc_path, building_id_filter, bbox_epsg, transform.as_mut().map(|f| f as _), &opts)
        .map_err(PyValueError::new_err)
}

/// Parse CityJSON (file, files, or document text) and write one IFC. `mode` is `"typed"` or `"mesh"`.
///
/// `psets` takes `bimfabrikhh_core_rs.psets.PsetSpec` objects (or their dicts).
/// Omit it and the IFC is written without property sets.
/// `ifc_types` maps surface types to IFC entities. Omit it and every product is
/// `IfcBuildingElementProxy`.
#[pyfunction]
#[pyo3(signature = (source, ifc_path, building_id_filter=None, mode="typed", project_name="bimfabrikhh_core_rs", site_name="Site", crs=None, styles=true, cad_layer=false, owner_history=true, bbox_epsg=None, transform=None, basepoint=None, psets=None, ifc_types=None, colors=None, owner=None, progress=false, basepoint_psets=None, basepoint_size=None))]
#[allow(clippy::too_many_arguments)]
fn cityjson_to_ifc(
    source: Bound<'_, PyAny>,
    ifc_path: &str,
    building_id_filter: Option<&str>,
    mode: &str,
    project_name: &str,
    site_name: &str,
    crs: Option<Bound<'_, PyAny>>,
    styles: bool,
    cad_layer: bool,
    owner_history: bool,
    bbox_epsg: Bbox,
    transform: Option<Bound<'_, PyAny>>,
    basepoint: Option<Bound<'_, PyTuple>>,
    psets: Option<Bound<'_, PyAny>>,
    ifc_types: Option<Bound<'_, PyAny>>,
    colors: Option<Bound<'_, PyAny>>,
    owner: Option<Bound<'_, PyAny>>,
    progress: bool,
    basepoint_psets: Option<Bound<'_, PyAny>>,
    basepoint_size: Option<f64>,
) -> PyResult<String> {
    let src = cityjson_source_from_py(&source)?;
    let opts = export_options(
        mode,
        project_name,
        site_name,
        crs.as_ref(),
        styles,
        cad_layer,
        owner_history,
        basepoint,
        psets.as_ref(),
        ifc_types.as_ref(),
        colors.as_ref(),
        owner.as_ref(),
        progress,
        basepoint_psets.as_ref(),
        basepoint_size,
    )?;
    let mut transform = transform.as_ref().map(xy_transform_from_py);
    api::cityjson_to_ifc(&src, ifc_path, building_id_filter, bbox_epsg, transform.as_mut().map(|f| f as _), &opts)
        .map_err(PyValueError::new_err)
}

/// Trunk + crown meshes → one IFC4 STEP file. Python still prepares records.
///
/// Each tree dict: ``name``, ``position`` ``(x, y)`` or ``(x, y, z)``,
/// ``trunk_radius``, ``trunk_height``, ``crown_radius``, ``detail``, ``segments``,
/// ``include_crown`` / ``is_stump``, optional ``attributes`` (source fields for
/// ``psets`` specs), optional raw ``psets``, optional colours.
///
/// ``trunk_height`` is the cylinder height, not the Stammhöhe: the crown sphere is
/// centred on the cylinder top, so pass Stammhöhe + ``crown_radius`` (BIMFabrikHH_core
/// ``TreeDimensions.trunk_cylinder_height``). For stumps it is the stump height.
///
/// ``psets`` takes ``bimfabrikhh_core_rs.psets.PsetSpec`` objects (or their dicts),
/// same as ``gml_to_ifc``. Omit it and no property sets are written.
///
/// ``drape_vertices`` is optional ``[[x, y, z], ...]``. With ``drape_faces``
/// rust interpolates Z on the containing triangle (same mesh as the DGM IFC).
/// Without faces it uses the nearest vertex. Omit both and rust does not
/// touch height.
#[pyfunction]
#[pyo3(signature = (trees, ifc_path, project_name="Trees_Generic_Project", site_name="Trees_Generic_Site", crs=None, styles=true, cad_layer=true, owner_history=true, basepoint=None, psets=None, owner=None, progress=false, drape_vertices=None, drape_faces=None, basepoint_psets=None, basepoint_size=None))]
#[allow(clippy::too_many_arguments)]
fn trees_to_ifc(
    trees: Bound<'_, PyList>,
    ifc_path: &str,
    project_name: &str,
    site_name: &str,
    crs: Option<Bound<'_, PyAny>>,
    styles: bool,
    cad_layer: bool,
    owner_history: bool,
    basepoint: Option<Bound<'_, PyTuple>>,
    psets: Option<Bound<'_, PyAny>>,
    owner: Option<Bound<'_, PyAny>>,
    progress: bool,
    drape_vertices: Option<Bound<'_, PyAny>>,
    drape_faces: Option<Bound<'_, PyAny>>,
    basepoint_psets: Option<Bound<'_, PyAny>>,
    basepoint_size: Option<f64>,
) -> PyResult<String> {
    let parsed = trees_from_py(&trees)?;
    let drape = match drape_vertices {
        Some(v) if !v.is_none() => Some(xyz_list_from_py(&v)?),
        _ => None,
    };
    let faces = match drape_faces {
        Some(v) if !v.is_none() => Some(faces_from_py(&v)?),
        _ => None,
    };
    let opts = export_options(
        "mesh",
        project_name,
        site_name,
        crs.as_ref(),
        styles,
        cad_layer,
        owner_history,
        basepoint,
        psets.as_ref(),
        None,
        None,
        owner.as_ref(),
        progress,
        basepoint_psets.as_ref(),
        basepoint_size,
    )?;
    api::trees_to_ifc(&parsed, ifc_path, &opts, drape.as_deref(), faces.as_deref()).map_err(PyValueError::new_err)
}

/// One triangulated DGM mesh → IFC4 STEP. Python still samples / Delaunay.
///
/// ``vertices`` is ``[[x, y, z], ...]``. ``faces`` is ``[[i, j, k], ...]``
/// (0-based, same as SciPy / BIMFabrik ``TerrainMesh``).
/// ``psets`` takes ``PsetSpec`` objects (or dicts). Omit it and no property
/// sets are written.
#[pyfunction]
#[pyo3(signature = (vertices, faces, ifc_path, name="DGM", project_name="DGM", site_name="Hamburg_Site", crs=None, styles=true, cad_layer=true, owner_history=true, basepoint=None, psets=None, owner=None, color=None, layer="_BIM_DGM_Gelaende", attributes=None, progress=false, basepoint_psets=None, basepoint_size=None))]
#[allow(clippy::too_many_arguments)]
fn terrain_to_ifc(
    vertices: Bound<'_, PyAny>,
    faces: Bound<'_, PyAny>,
    ifc_path: &str,
    name: &str,
    project_name: &str,
    site_name: &str,
    crs: Option<Bound<'_, PyAny>>,
    styles: bool,
    cad_layer: bool,
    owner_history: bool,
    basepoint: Option<Bound<'_, PyTuple>>,
    psets: Option<Bound<'_, PyAny>>,
    owner: Option<Bound<'_, PyAny>>,
    color: Option<Bound<'_, PyAny>>,
    layer: &str,
    attributes: Option<Bound<'_, PyAny>>,
    progress: bool,
    basepoint_psets: Option<Bound<'_, PyAny>>,
    basepoint_size: Option<f64>,
) -> PyResult<String> {
    let mesh = terrain_from_py(
        &vertices,
        &faces,
        name,
        color.as_ref(),
        Some(layer),
        attributes.as_ref(),
    )?;
    let opts = export_options(
        "mesh",
        project_name,
        site_name,
        crs.as_ref(),
        styles,
        cad_layer,
        owner_history,
        basepoint,
        psets.as_ref(),
        None,
        None,
        owner.as_ref(),
        progress,
        basepoint_psets.as_ref(),
        basepoint_size,
    )?;
    api::terrain_to_ifc(&mesh, ifc_path, &opts).map_err(PyValueError::new_err)
}

/// Many guided DGM land-use parts → one IFC4 STEP, each part its own styled
/// `IfcBuildingElementProxy` contained in the site. Same meshing/splitting is
/// done in Python (`TerrainGenericApp` helpers); Rust only writes STEP.
///
/// ``parts`` is a list of dicts: ``name``, ``vertices`` (``[[x, y, z], ...]``),
/// ``faces`` (``[[i, j, k], ...]``, 0-based), and optional ``color``,
/// ``layer``, ``attributes`` and ``psets`` (``{pset_name: {property: value}}``).
/// All parts share one local origin (``basepoint`` or the combined lower-left).
/// ``psets`` here (like ``terrain_to_ifc``) is optional ``PsetSpec`` templates
/// applied to every part on top of its own concrete psets.
#[pyfunction]
#[pyo3(signature = (parts, ifc_path, project_name="DGM", site_name="Hamburg_Site", crs=None, styles=true, cad_layer=true, owner_history=true, basepoint=None, psets=None, owner=None, progress=false, basepoint_psets=None, basepoint_size=None))]
#[allow(clippy::too_many_arguments)]
fn terrain_parts_to_ifc(
    parts: Bound<'_, PyList>,
    ifc_path: &str,
    project_name: &str,
    site_name: &str,
    crs: Option<Bound<'_, PyAny>>,
    styles: bool,
    cad_layer: bool,
    owner_history: bool,
    basepoint: Option<Bound<'_, PyTuple>>,
    psets: Option<Bound<'_, PyAny>>,
    owner: Option<Bound<'_, PyAny>>,
    progress: bool,
    basepoint_psets: Option<Bound<'_, PyAny>>,
    basepoint_size: Option<f64>,
) -> PyResult<String> {
    let parsed = terrain_parts_from_py(&parts)?;
    let opts = export_options(
        "mesh",
        project_name,
        site_name,
        crs.as_ref(),
        styles,
        cad_layer,
        owner_history,
        basepoint,
        psets.as_ref(),
        None,
        None,
        owner.as_ref(),
        progress,
        basepoint_psets.as_ref(),
        basepoint_size,
    )?;
    api::terrain_parts_to_ifc(&parsed, ifc_path, &opts).map_err(PyValueError::new_err)
}
