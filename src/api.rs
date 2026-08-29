//! Pure-Rust pipeline: parse → export → IFC4 STEP.
//!
//! No pyo3 and no HTTP live here, so the whole conversion can be driven and
//! tested from plain Rust. The Python layer in [`crate::bindings`] only marshals
//! arguments into these calls.

use std::collections::HashMap;

use crate::cityjson::{parse_cityjson_file, parse_cityjson_str};
use crate::color::Colors;
use crate::geom::{buildings_to_export, ExportConfig};
use crate::ifc::{write_ifc_file, OwnerIdentity, WriteOptions};
use crate::parse::{parse_gml_file, BboxUtm};
use crate::psetspec::PsetSpec;
use crate::types::{BuildingExport, ParsedBuilding, Product};

pub fn is_mesh(mode: &str) -> bool {
    mode.eq_ignore_ascii_case("mesh")
}

/// Everything the IFC writer needs beyond the geometry itself.
pub struct ExportOptions {
    pub mode: String,
    pub project_name: String,
    pub site_name: String,
    pub epsg: i32,
    pub styles: bool,
    pub owner_history: bool,
    pub owner: OwnerIdentity,
    pub basepoint: Option<(f64, f64)>,
    /// Empty means the IFC is written without property sets.
    pub psets: Vec<PsetSpec>,
    /// Empty means every product is `IfcBuildingElementProxy`.
    pub ifc_types: HashMap<String, String>,
    /// `Defaults` uses the table in [`crate::color`]. Caller maps override keys.
    pub colors: Colors,
}

impl ExportOptions {
    fn export_config(&self) -> ExportConfig<'_> {
        ExportConfig {
            mode: &self.mode,
            styles: self.styles,
            colors: &self.colors,
            specs: &self.psets,
            ifc_types: &self.ifc_types,
        }
    }

    fn write_options(&self) -> WriteOptions {
        WriteOptions {
            project_name: self.project_name.clone(),
            site_name: self.site_name.clone(),
            epsg: self.epsg,
            styles: self.styles,
            owner_history: self.owner_history,
            owner: self.owner.clone(),
            basepoint: self.basepoint,
            city_model: is_mesh(&self.mode),
        }
    }
}

/// A CityJSON input: either a document already in memory or one or more files.
pub enum CityJsonSource {
    Text(String),
    Paths(Vec<String>),
}

pub fn parse_gml_sources(
    paths: &[String],
    building_id_filter: Option<&str>,
    bbox_epsg: Option<BboxUtm>,
) -> Result<Vec<ParsedBuilding>, String> {
    let mut buildings = Vec::new();
    for path in paths {
        buildings.extend(parse_gml_file(path, building_id_filter, bbox_epsg)?);
    }
    if buildings.is_empty() {
        return Err("no buildings parsed from CityGML".into());
    }
    Ok(buildings)
}

pub fn parse_cityjson_source(
    source: &CityJsonSource,
    building_id_filter: Option<&str>,
    bbox_epsg: Option<BboxUtm>,
) -> Result<Vec<ParsedBuilding>, String> {
    let mut buildings = Vec::new();
    match source {
        CityJsonSource::Text(text) => {
            buildings = parse_cityjson_str(text, building_id_filter, bbox_epsg)?;
        }
        CityJsonSource::Paths(paths) => {
            if paths.is_empty() {
                return Err("cityjson source list is empty".into());
            }
            for path in paths {
                buildings.extend(parse_cityjson_file(path, building_id_filter, bbox_epsg)?);
            }
        }
    }
    if buildings.is_empty() {
        return Err("no buildings parsed from CityJSON".into());
    }
    Ok(buildings)
}

/// Parse CityGML (one file or many) and write a single IFC.
pub fn gml_to_ifc(
    paths: &[String],
    ifc_path: &str,
    building_id_filter: Option<&str>,
    bbox_epsg: Option<BboxUtm>,
    opts: &ExportOptions,
) -> Result<String, String> {
    let buildings = parse_gml_sources(paths, building_id_filter, bbox_epsg)?;
    write_buildings(&buildings, ifc_path, opts)
}

/// Parse CityJSON (text, one file, or many) and write a single IFC.
pub fn cityjson_to_ifc(
    source: &CityJsonSource,
    ifc_path: &str,
    building_id_filter: Option<&str>,
    bbox_epsg: Option<BboxUtm>,
    opts: &ExportOptions,
) -> Result<String, String> {
    let buildings = parse_cityjson_source(source, building_id_filter, bbox_epsg)?;
    write_buildings(&buildings, ifc_path, opts)
}

/// Write IFC4 STEP from products that were built outside this crate.
pub fn products_to_ifc(
    products: Vec<Product>,
    ifc_path: &str,
    opts: &ExportOptions,
) -> Result<String, String> {
    write_ifc_file(&group_products(products), ifc_path, &opts.write_options())?;
    Ok(ifc_path.to_string())
}

fn write_buildings(
    buildings: &[ParsedBuilding],
    ifc_path: &str,
    opts: &ExportOptions,
) -> Result<String, String> {
    let export = buildings_to_export(buildings, &opts.export_config());
    write_ifc_file(&export, ifc_path, &opts.write_options())?;
    Ok(ifc_path.to_string())
}

pub fn group_products(products: Vec<Product>) -> Vec<BuildingExport> {
    let mut order: Vec<String> = Vec::new();
    let mut map: HashMap<String, Vec<Product>> = HashMap::new();
    for p in products {
        map.entry(p.parent.clone()).or_insert_with(|| {
            order.push(p.parent.clone());
            Vec::new()
        });
        map.get_mut(&p.parent).unwrap().push(p);
    }
    order
        .into_iter()
        .map(|id| BuildingExport {
            display_name: id.clone(),
            id: id.clone(),
            psets: Vec::new(),
            products: map.remove(&id).unwrap_or_default(),
        })
        .collect()
}
