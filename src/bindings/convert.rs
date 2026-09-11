//! Python ↔ Rust marshalling. Nothing here decides what the pipeline does.

use std::collections::HashMap;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyList, PyTuple};

use crate::api::CityJsonSource;
use crate::color::{Colors, CROWN_RGB, TERRAIN_RGB, TRUNK_RGB};
use crate::ifc::OwnerIdentity;
use crate::psetspec::{AttrKey, Catalog, PsetSpec, QtyKey, Scope, Value};
use crate::terrain::{TerrainInput, TERRAIN_LAYER};
use crate::tree::TreeInput;
use crate::types::{upsert_pset, Face, ParsedBuilding, Product, Pset};

pub fn building_to_py(py: Python<'_>, b: &ParsedBuilding) -> PyResult<PyObject> {
    let d = PyDict::new(py);
    d.set_item("id", &b.id)?;
    d.set_item("gml_name", &b.gml_name)?;
    d.set_item("lod", &b.lod)?;
    let attrs = PyDict::new(py);
    attrs.set_item("function", &b.attributes.function)?;
    attrs.set_item("measured_height", b.attributes.measured_height)?;
    attrs.set_item("storeys_above_ground", b.attributes.storeys_above_ground)?;
    attrs.set_item("roof_type", &b.attributes.roof_type)?;
    for (k, v) in &b.attributes.extra {
        attrs.set_item(k, v)?;
    }
    d.set_item("attributes", attrs)?;
    let bounds = PyList::empty(py);
    for bd in &b.boundaries {
        let e = PyDict::new(py);
        e.set_item("ring", bd.ring.iter().map(|p| (p[0], p[1], p[2])).collect::<Vec<_>>())?;
        e.set_item(
            "interior_rings",
            bd.interiors
                .iter()
                .map(|r| r.iter().map(|p| (p[0], p[1], p[2])).collect::<Vec<_>>())
                .collect::<Vec<_>>(),
        )?;
        e.set_item("surface_type", &bd.surface_type)?;
        e.set_item("source_part_id", &bd.source_part_id)?;
        bounds.append(e)?;
    }
    d.set_item("boundaries", bounds)?;
    Ok(d.into())
}

pub fn dict_req<'py, T: FromPyObject<'py>>(item: &Bound<'py, PyDict>, key: &str) -> PyResult<T> {
    item.get_item(key)?
        .ok_or_else(|| PyValueError::new_err(format!("missing key '{key}'")))?
        .extract()
}

pub fn dict_or<'py, T: FromPyObject<'py>>(item: &Bound<'py, PyDict>, key: &str, default: T) -> T {
    item.get_item(key)
        .ok()
        .flatten()
        .and_then(|v| v.extract().ok())
        .unwrap_or(default)
}

pub fn product_from_py(item: &Bound<'_, PyDict>) -> PyResult<Product> {
    let name: String = dict_req(item, "name")?;
    let ifc_type: String = dict_or(item, "ifc_type", "IfcBuildingElementProxy".to_string());
    let parent: String = dict_or(item, "parent", name.clone());
    let vertices_raw: Vec<Vec<f64>> = dict_req(item, "vertices")?;
    let vertices = vertices_raw
        .into_iter()
        .map(|p| if p.len() < 3 { [0.0, 0.0, 0.0] } else { [p[0], p[1], p[2]] })
        .collect();
    let mut faces = Vec::new();
    let faces_obj = item.get_item("faces")?.ok_or_else(|| PyValueError::new_err("missing key 'faces'"))?;
    if let Ok(simple) = faces_obj.extract::<Vec<Vec<u32>>>() {
        faces.extend(simple.into_iter().map(Face::Simple));
    } else if let Ok(list) = faces_obj.downcast::<PyList>() {
        for face in list.iter() {
            if let Ok(idx) = face.extract::<Vec<u32>>() {
                faces.push(Face::Simple(idx));
            } else if let Ok(d) = face.downcast::<PyDict>() {
                let outer: Vec<u32> = dict_req(d, "outer")?;
                let inners: Vec<Vec<u32>> = dict_or(d, "inners", Vec::new());
                faces.push(Face::WithVoids { outer, inners });
            }
        }
    }
    let psets = match item.get_item("psets")? {
        Some(v) if !v.is_none() => psets_from_py(&v)?,
        _ => Vec::new(),
    };
    Ok(Product {
        name,
        ifc_type,
        parent,
        vertices,
        faces,
        psets,
        color: color_from_py(item)?,
        layer: None,
        extra: Vec::new(),
        placement: None,
    })
}

fn color_from_py(item: &Bound<'_, PyDict>) -> PyResult<Option<[f64; 3]>> {
    match item.get_item("color")? {
        Some(v) if !v.is_none() => Ok(Some(rgb_from_py(&v)?)),
        _ => Ok(None),
    }
}

fn rgb_from_py(v: &Bound<'_, PyAny>) -> PyResult<[f64; 3]> {
    let nums: Vec<f64> = v.extract().map_err(|_| {
        PyValueError::new_err("color must be a sequence of 3 numbers in 0..1 (r, g, b)")
    })?;
    if nums.len() != 3 {
        return Err(PyValueError::new_err(format!(
            "color needs 3 components, got {}",
            nums.len()
        )));
    }
    Ok([nums[0], nums[1], nums[2]])
}

/// `None` → Rust defaults. A 3-tuple is one colour for everything. A dict maps
/// surface type (or `"Building"`) to an RGB triple; missing keys keep the default.
pub fn colors_from_py(obj: Option<&Bound<'_, PyAny>>) -> PyResult<Colors> {
    let Some(obj) = obj.filter(|o| !o.is_none()) else {
        return Ok(Colors::Defaults);
    };
    if let Ok(d) = obj.downcast::<PyDict>() {
        let mut map = HashMap::new();
        for (key, value) in d.iter() {
            let key: String = key.extract()?;
            map.insert(key, rgb_from_py(&value)?);
        }
        return Ok(if map.is_empty() {
            Colors::Defaults
        } else {
            Colors::Map(map)
        });
    }
    Ok(Colors::Uniform(rgb_from_py(obj)?))
}

fn overlay_owner_field(dst: &mut String, d: &Bound<'_, PyDict>, keys: &[&str]) -> PyResult<()> {
    for key in keys {
        if let Some(v) = opt_item(d, key) {
            let s = any_to_string(&v)?;
            if !s.is_empty() {
                *dst = s;
                return Ok(());
            }
        }
    }
    Ok(())
}

/// `None` keeps the Rust default (FILE_NAME author = BIM-Leitstelle, LGV Hamburg). A dict or `to_wire()`
/// object overlays any keys that are present.
pub fn owner_from_py(obj: Option<&Bound<'_, PyAny>>) -> PyResult<OwnerIdentity> {
    let mut owner = OwnerIdentity::default();
    let Some(obj) = obj.filter(|o| !o.is_none()) else {
        return Ok(owner);
    };
    let d = if obj.downcast::<PyDict>().is_ok() {
        as_dict(obj, "owner")?
    } else if obj.hasattr("to_wire")? {
        as_dict(&obj.call_method0("to_wire")?, "to_wire()")?
    } else if obj.hasattr("model_dump")? {
        as_dict(&obj.call_method0("model_dump")?, "model_dump()")?
    } else {
        return Err(PyValueError::new_err(
            "owner must be a dict or an object with to_wire()/model_dump() \
             (see bimfabrikhh_core_rs.owner.OwnerIdentity)",
        ));
    };
    overlay_owner_field(&mut owner.family_name, &d, &["family_name", "family", "FamilyName"])?;
    overlay_owner_field(&mut owner.given_name, &d, &["given_name", "given", "GivenName"])?;
    overlay_owner_field(&mut owner.organization, &d, &["organization", "org", "Organisation"])?;
    overlay_owner_field(&mut owner.application, &d, &["application", "app", "ApplicationFullName"])?;
    overlay_owner_field(&mut owner.version, &d, &["version", "Version"])?;
    overlay_owner_field(&mut owner.identifier, &d, &["identifier", "id", "ApplicationIdentifier"])?;
    overlay_owner_field(
        &mut owner.preprocessor,
        &d,
        &["preprocessor", "preprocessor_version"],
    )?;
    overlay_owner_field(
        &mut owner.originating_system,
        &d,
        &["originating_system", "system"],
    )?;
    overlay_owner_field(
        &mut owner.authorization,
        &d,
        &["authorization", "authorisation"],
    )?;
    Ok(owner)
}

/// Product psets arrive as `{pset_name: {property: value}}`. Python dicts keep
/// insertion order, so that order carries through to the written IFC.
fn attrs_from_py(obj: &Bound<'_, PyAny>) -> PyResult<HashMap<String, String>> {
    let d = as_dict(obj, "attributes")?;
    let mut out = HashMap::new();
    for (key, value) in d.iter() {
        out.insert(key.extract()?, any_to_string(&value)?);
    }
    Ok(out)
}

fn psets_from_py(obj: &Bound<'_, PyAny>) -> PyResult<Vec<Pset>> {
    let mut out = Vec::new();
    for (name, props) in as_dict(obj, "psets")?.iter() {
        let name: String = name.extract()?;
        let props = as_dict(&props, "a pset")?;
        let mut properties = Vec::with_capacity(props.len());
        for (key, value) in props.iter() {
            properties.push((key.extract()?, any_to_string(&value)?));
        }
        upsert_pset(&mut out, Pset::new(name, properties));
    }
    Ok(out)
}

pub fn trees_from_py(trees: &Bound<'_, PyList>) -> PyResult<Vec<TreeInput>> {
    let mut out = Vec::new();
    for (i, item) in trees.iter().enumerate() {
        let d = item
            .downcast::<PyDict>()
            .map_err(|_| PyValueError::new_err("trees_to_ifc expects a list of dicts"))?;
        let name: String = dict_or(d, "name", format!("Baum_{:03}", i + 1));
        let pos = xyz_from_py(d, "position")?;
        let include_crown = !dict_or(d, "is_stump", false) && dict_or(d, "include_crown", true);
        let psets = match d.get_item("psets")? {
            Some(v) if !v.is_none() => psets_from_py(&v)?,
            _ => Vec::new(),
        };
        let attributes = match d.get_item("attributes")? {
            Some(v) if !v.is_none() => attrs_from_py(&v)?,
            _ => HashMap::new(),
        };
        out.push(TreeInput {
            name,
            position: pos,
            trunk_radius: dict_or(d, "trunk_radius", 0.3),
            trunk_height: dict_or(d, "trunk_height", 4.0),
            trunk_segments: dict_or(d, "segments", dict_or(d, "trunk_segments", 8u32)),
            crown_radius: dict_or(d, "crown_radius", 2.5),
            crown_detail: dict_or(d, "detail", dict_or(d, "crown_detail", 1u32)),
            include_crown,
            psets,
            attributes,
            trunk_color: rgb_or(d, "trunk_color", TRUNK_RGB)?,
            crown_color: rgb_or(d, "crown_color", CROWN_RGB)?,
        });
    }
    Ok(out)
}

pub fn xyz_list_from_py(vertices: &Bound<'_, PyAny>) -> PyResult<Vec<[f64; 3]>> {
    let raw: Vec<Vec<f64>> = vertices
        .extract()
        .map_err(|_| PyValueError::new_err("vertices must be a list of [x, y, z]"))?;
    Ok(raw
        .into_iter()
        .map(|p| {
            if p.len() < 3 {
                [0.0, 0.0, 0.0]
            } else {
                [p[0], p[1], p[2]]
            }
        })
        .collect())
}

pub fn faces_from_py(faces: &Bound<'_, PyAny>) -> PyResult<Vec<Face>> {
    if let Ok(simple) = faces.extract::<Vec<Vec<u32>>>() {
        return Ok(simple.into_iter().map(Face::Simple).collect());
    }
    if let Ok(list) = faces.downcast::<PyList>() {
        let mut mesh_faces = Vec::new();
        for face in list.iter() {
            if let Ok(idx) = face.extract::<Vec<u32>>() {
                mesh_faces.push(Face::Simple(idx));
            } else {
                return Err(PyValueError::new_err("faces must be lists of vertex indices"));
            }
        }
        return Ok(mesh_faces);
    }
    Err(PyValueError::new_err("faces must be a list of index lists"))
}

fn xyz_from_py(d: &Bound<'_, PyDict>, key: &str) -> PyResult<[f64; 3]> {
    let v = d
        .get_item(key)?
        .ok_or_else(|| PyValueError::new_err(format!("missing key '{key}'")))?;
    let nums: Vec<f64> = v.extract().map_err(|_| {
        PyValueError::new_err(format!("{key} must be (x, y) or (x, y, z)"))
    })?;
    match nums.len() {
        2 => Ok([nums[0], nums[1], 0.0]),
        3 => Ok([nums[0], nums[1], nums[2]]),
        n => Err(PyValueError::new_err(format!("{key} needs 2 or 3 numbers, got {n}"))),
    }
}

fn rgb_or(d: &Bound<'_, PyDict>, key: &str, default: [f64; 3]) -> PyResult<[f64; 3]> {
    match d.get_item(key)? {
        Some(v) if !v.is_none() => rgb_style_from_py(&v),
        _ => Ok(default),
    }
}

fn rgb_style_from_py(v: &Bound<'_, PyAny>) -> PyResult<[f64; 3]> {
    let nums: Vec<f64> = v
        .extract()
        .map_err(|_| PyValueError::new_err("color must be 3 numbers (0..1 or 0..255)"))?;
    if nums.len() != 3 {
        return Err(PyValueError::new_err(format!("color needs 3 components, got {}", nums.len())));
    }
    if nums.iter().any(|x| *x > 1.0) {
        Ok([nums[0] / 255.0, nums[1] / 255.0, nums[2] / 255.0])
    } else {
        Ok([nums[0], nums[1], nums[2]])
    }
}

pub fn terrain_from_py(
    vertices: &Bound<'_, PyAny>,
    faces: &Bound<'_, PyAny>,
    name: &str,
    color: Option<&Bound<'_, PyAny>>,
    layer: Option<&str>,
    attributes: Option<&Bound<'_, PyAny>>,
) -> PyResult<TerrainInput> {
    let verts = xyz_list_from_py(vertices)?;
    let mesh_faces = faces_from_py(faces)?;
    let rgb = match color {
        Some(v) if !v.is_none() => rgb_style_from_py(v)?,
        _ => TERRAIN_RGB,
    };
    let attributes = match attributes {
        Some(v) if !v.is_none() => attrs_from_py(v)?,
        _ => HashMap::new(),
    };
    Ok(TerrainInput {
        name: if name.is_empty() { "DGM".into() } else { name.into() },
        vertices: verts,
        faces: mesh_faces,
        color: rgb,
        layer: Some(layer.unwrap_or(TERRAIN_LAYER).into()),
        attributes,
        psets: Vec::new(),
        placement: None,
    })
}

/// A list of land-use part dicts → `TerrainInput`s for `terrain_parts_to_ifc`.
///
/// Each dict: `name`, `vertices`, `faces`, optional `color`, `layer`,
/// `attributes`, and `psets` (`{pset_name: {property: value}}`, same shape as
/// `write_ifc` products).
pub fn terrain_parts_from_py(parts: &Bound<'_, PyList>) -> PyResult<Vec<TerrainInput>> {
    let mut out = Vec::with_capacity(parts.len());
    for item in parts.iter() {
        let d = item
            .downcast::<PyDict>()
            .map_err(|_| PyValueError::new_err("terrain_parts_to_ifc expects a list of dicts"))?;
        let name: String = dict_req(d, "name")?;
        let vertices = xyz_list_from_py(&req(d, "vertices")?)?;
        let faces = faces_from_py(&req(d, "faces")?)?;
        let color = match d.get_item("color")? {
            Some(v) if !v.is_none() => rgb_style_from_py(&v)?,
            _ => TERRAIN_RGB,
        };
        let layer: Option<String> = match d.get_item("layer")? {
            Some(v) if !v.is_none() => Some(v.extract()?),
            _ => None,
        };
        let attributes = match d.get_item("attributes")? {
            Some(v) if !v.is_none() => attrs_from_py(&v)?,
            _ => HashMap::new(),
        };
        let psets = match d.get_item("psets")? {
            Some(v) if !v.is_none() => psets_from_py(&v)?,
            _ => Vec::new(),
        };
        out.push(TerrainInput {
            name: if name.is_empty() { "DGM".into() } else { name },
            vertices,
            faces,
            color,
            layer: Some(layer.unwrap_or_else(|| TERRAIN_LAYER.into())),
            attributes,
            psets,
            placement: None,
        });
    }
    Ok(out)
}

pub fn products_from_py(products: &Bound<'_, PyList>) -> PyResult<Vec<Product>> {
    let mut parsed = Vec::new();
    for item in products.iter() {
        let d = item
            .downcast::<PyDict>()
            .map_err(|_| PyValueError::new_err("write_ifc expects a list of dicts"))?;
        parsed.push(product_from_py(d)?);
    }
    Ok(parsed)
}

pub fn basepoint_xy(basepoint: Option<Bound<'_, PyTuple>>) -> PyResult<Option<(f64, f64)>> {
    match basepoint {
        Some(t) if t.len() >= 2 => Ok(Some((t.get_item(0)?.extract()?, t.get_item(1)?.extract()?))),
        _ => Ok(None),
    }
}

pub fn gml_paths_from_py(obj: &Bound<'_, PyAny>) -> PyResult<Vec<String>> {
    if let Ok(s) = obj.extract::<String>() {
        return Ok(vec![s]);
    }
    if let Ok(list) = obj.extract::<Vec<String>>() {
        if list.is_empty() {
            return Err(PyValueError::new_err("gml_path list is empty"));
        }
        return Ok(list);
    }
    Err(PyValueError::new_err("gml_path must be a str or a list of str"))
}

fn err<T: std::fmt::Display>(e: T) -> PyErr {
    PyValueError::new_err(e.to_string())
}

fn req<'py>(d: &Bound<'py, PyDict>, key: &str) -> PyResult<Bound<'py, PyAny>> {
    d.get_item(key)?
        .ok_or_else(|| PyValueError::new_err(format!("pset spec is missing key '{key}'")))
}

fn as_dict<'py>(v: &Bound<'py, PyAny>, what: &str) -> PyResult<Bound<'py, PyDict>> {
    v.downcast::<PyDict>()
        .cloned()
        .map_err(|_| PyValueError::new_err(format!("{what} must be a dict")))
}

/// Property values are written as text, so accept whatever Python offers.
fn any_to_string(v: &Bound<'_, PyAny>) -> PyResult<String> {
    v.str()?.extract()
}

fn opt_item<'py>(d: &Bound<'py, PyDict>, key: &str) -> Option<Bound<'py, PyAny>> {
    d.get_item(key).ok().flatten().filter(|v| !v.is_none())
}

fn catalog_from_py(v: Option<Bound<'_, PyAny>>) -> PyResult<Option<Catalog>> {
    let Some(v) = v else { return Ok(None) };
    if let Ok(name) = v.extract::<String>() {
        return Catalog::parse(&name).map(Some).map_err(err);
    }
    if let Ok(map) = v.extract::<HashMap<String, String>>() {
        return Ok(Some(Catalog::Custom(map)));
    }
    Err(PyValueError::new_err(
        "catalog must be 'funktion', 'dachform', or a dict of code -> label",
    ))
}

fn value_from_py(d: &Bound<'_, PyDict>) -> PyResult<Value> {
    let kind: String = dict_req(d, "kind")?;
    match kind.as_str() {
        "const" => Ok(Value::Const(any_to_string(&req(d, "value")?)?)),
        "attr" => Ok(Value::Attr {
            key: AttrKey::parse(&dict_req::<String>(d, "key")?).map_err(err)?,
            catalog: catalog_from_py(opt_item(d, "catalog"))?,
            decimals: dict_or(d, "decimals", None),
            default: opt_item(d, "default")
                .map(|v| any_to_string(&v))
                .transpose()?,
        }),
        "quantity" => Ok(Value::Quantity {
            key: QtyKey::parse(&dict_req::<String>(d, "key")?).map_err(err)?,
            decimals: dict_or(d, "decimals", None),
        }),
        other => Err(PyValueError::new_err(format!(
            "unknown property kind '{other}'; expected 'const', 'attr' or 'quantity'"
        ))),
    }
}

/// Properties may be a dict (Python keeps insertion order) or a list of
/// `[name, value]` pairs.
fn properties_from_py(obj: &Bound<'_, PyAny>) -> PyResult<Vec<(String, Value)>> {
    let mut out = Vec::new();
    if let Ok(d) = obj.downcast::<PyDict>() {
        for (name, value) in d.iter() {
            let name: String = name.extract()?;
            out.push((name, value_from_py(&as_dict(&value, "property value")?)?));
        }
        return Ok(out);
    }
    // A pair may be a tuple or, after a JSON round trip, a list.
    for item in obj.try_iter()? {
        let pair = item?;
        let name: String = pair.get_item(0)?.extract()?;
        let value = pair.get_item(1)?;
        out.push((name, value_from_py(&as_dict(&value, "property value")?)?));
    }
    Ok(out)
}

/// A wire spec is a plain dict, or any object exposing `to_wire()` /
/// `model_dump()` such as a pydantic `PsetSpec`. That call happens once per
/// spec per run, never per building.
fn wire_dict<'py>(item: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyDict>> {
    if let Ok(d) = item.downcast::<PyDict>() {
        return Ok(d.clone());
    }
    for method in ["to_wire", "model_dump"] {
        if item.hasattr(method)? {
            let out = item.call_method0(method)?;
            return as_dict(&out, &format!("{method}()"));
        }
    }
    Err(PyValueError::new_err(
        "psets entries must be dicts or objects with to_wire()/model_dump() \
         (see bimfabrikhh_core_rs.psets.PsetSpec)",
    ))
}

fn spec_from_py(item: &Bound<'_, PyAny>) -> PyResult<PsetSpec> {
    let d = wire_dict(item)?;
    let name: String = req(&d, "name")?.extract()?;
    let scope = Scope::parse(&dict_or(&d, "scope", "building".to_string())).map_err(err)?;
    let properties = properties_from_py(&req(&d, "properties")?)?;
    if properties.is_empty() {
        return Err(PyValueError::new_err(format!(
            "pset spec '{name}' has no properties"
        )));
    }
    Ok(PsetSpec { name, scope, properties })
}

/// Surface type → IFC entity. A plain dict, or an object with `to_wire()` /
/// `model_dump()` such as `bimfabrikhh_core_rs.ifc_types.IfcTypeMap`.
pub fn ifc_types_from_py(obj: Option<&Bound<'_, PyAny>>) -> PyResult<HashMap<String, String>> {
    let Some(obj) = obj.filter(|o| !o.is_none()) else {
        return Ok(HashMap::new());
    };
    let d = if obj.downcast::<PyDict>().is_ok() {
        as_dict(obj, "ifc_types")?
    } else if obj.hasattr("to_wire")? {
        as_dict(&obj.call_method0("to_wire")?, "to_wire()")?
    } else if obj.hasattr("model_dump")? {
        let dumped = obj.call_method0("model_dump")?;
        let outer = as_dict(&dumped, "model_dump()")?;
        match opt_item(&outer, "types") {
            Some(inner) => as_dict(&inner, "types")?,
            None => outer,
        }
    } else {
        return Err(PyValueError::new_err(
            "ifc_types must be a dict or an object with to_wire()/model_dump() \
             (see bimfabrikhh_core_rs.ifc_types.IfcTypeMap)",
        ));
    };
    let mut out = HashMap::new();
    for (key, value) in d.iter() {
        let key: String = key.extract()?;
        let value = any_to_string(&value)?;
        if key.is_empty() || value.is_empty() {
            return Err(PyValueError::new_err(
                "ifc_types entries must be non-empty surface type → Ifc entity names",
            ));
        }
        out.insert(key, value);
    }
    Ok(out)
}

pub fn pset_specs_from_py(obj: Option<&Bound<'_, PyAny>>) -> PyResult<Vec<PsetSpec>> {
    let Some(obj) = obj.filter(|o| !o.is_none()) else {
        return Ok(Vec::new());
    };
    // A single spec is a convenience shorthand for a one-element list.
    if obj.downcast::<PyDict>().is_ok() || obj.hasattr("to_wire")? {
        return Ok(vec![spec_from_py(obj)?]);
    }
    obj.try_iter()?
        .map(|item| spec_from_py(&item?))
        .collect()
}

/// A leading `{` marks a CityJSON document; anything else is a filesystem path.
pub fn cityjson_source_from_py(obj: &Bound<'_, PyAny>) -> PyResult<CityJsonSource> {
    if let Ok(s) = obj.extract::<String>() {
        if s.trim_start().starts_with('{') {
            return Ok(CityJsonSource::Texts(vec![s]));
        }
        return Ok(CityJsonSource::Paths(vec![s]));
    }
    if let Ok(list) = obj.extract::<Vec<String>>() {
        if list.iter().all(|s| s.trim_start().starts_with('{')) {
            return Ok(CityJsonSource::Texts(list));
        }
        return Ok(CityJsonSource::Paths(list));
    }
    Err(PyValueError::new_err("source must be a path, a list of paths, or CityJSON text"))
}
