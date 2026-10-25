//! Pure-Rust pipeline: parse → export → IFC4 STEP.
//!
//! No pyo3 and no HTTP live here, so the whole conversion can be driven and
//! tested from plain Rust. The Python layer in [`crate::bindings`] only marshals
//! arguments into these calls.

use std::collections::HashMap;
use std::fs;
use std::time::Instant;

use rayon::prelude::*;

use crate::cityjson::{parse_cityjson_json, parse_cityjson_value};
use crate::color::Colors;
use crate::geom::{buildings_to_export, ExportConfig};
use crate::ifc::{write_ifc_file, OwnerIdentity, ProjectedCrs, WriteOptions};
use crate::parse::{parse_gml_file, BboxUtm};
use crate::psetspec::{self, PsetSpec, Scope};
use crate::srs::{cityjson_declared_crs, common_declared_crs, gml_declared_crs, reconcile_crs, DeclaredCrs};
use crate::terrain::{elevation_ok, terrain_attr_map, terrain_to_product, TerrainInput};
use crate::tree::{drape_trees, tree_attr_map, trees_to_products, TreeInput};
use crate::types::{upsert_pset, BuildingExport, Face, ParsedBuilding, Product};

pub fn is_mesh(mode: &str) -> bool {
    mode.eq_ignore_ascii_case("mesh")
}

/// Everything the IFC writer needs beyond the geometry itself.
pub struct ExportOptions {
    pub mode: String,
    pub project_name: String,
    pub site_name: String,
    /// `None` writes no georeferencing.
    pub crs: Option<ProjectedCrs>,
    pub styles: bool,
    /// Off by default. When true, products are assigned to `_BIM_Stadtmodell`.
    pub cad_layer: bool,
    pub owner_history: bool,
    pub owner: OwnerIdentity,
    pub basepoint: Option<(f64, f64)>,
    /// Empty means the IFC is written without property sets.
    pub psets: Vec<PsetSpec>,
    /// Empty means every product is `IfcBuildingElementProxy`.
    pub ifc_types: HashMap<String, String>,
    /// `Defaults` uses the table in [`crate::color`]. Caller maps override keys.
    pub colors: Colors,
    /// Stderr parse/write counters. Off by default.
    pub progress: bool,
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
            crs: self.crs.clone(),
            styles: self.styles,
            cad_layer: self.cad_layer,
            owner_history: self.owner_history,
            owner: self.owner.clone(),
            basepoint: self.basepoint,
            city_model: is_mesh(&self.mode),
            contain_in_site: false,
            progress: self.progress,
        }
    }
}

/// A CityJSON input: documents already in memory, or one or more files.
pub enum CityJsonSource {
    Texts(Vec<String>),
    Paths(Vec<String>),
}

pub fn parse_gml_sources(
    paths: &[String],
    building_id_filter: Option<&str>,
    bbox_epsg: Option<BboxUtm>,
    progress: bool,
) -> Result<Vec<ParsedBuilding>, String> {
    if paths.len() <= 1 {
        let mut buildings = Vec::new();
        for path in paths {
            buildings.extend(parse_gml_file(path, building_id_filter, bbox_epsg, progress)?);
        }
        if buildings.is_empty() {
            return Err("no buildings parsed from CityGML".into());
        }
        return Ok(buildings);
    }

    let filter = building_id_filter.map(str::to_owned);
    let groups: Vec<Vec<ParsedBuilding>> = paths
        .par_iter()
        .map(|path| parse_gml_file(path, filter.as_deref(), bbox_epsg, false))
        .collect::<Result<_, _>>()?;
    let buildings: Vec<ParsedBuilding> = groups.into_iter().flatten().collect();
    if buildings.is_empty() {
        return Err("no buildings parsed from CityGML".into());
    }
    if progress {
        eprintln!("  parse_gml: {:>5} buildings", buildings.len());
    }
    Ok(buildings)
}

/// Buildings of all documents, plus the CRS they declare in
/// `metadata.referenceSystem` (see [`common_declared_crs`]).
pub fn parse_cityjson_source(
    source: &CityJsonSource,
    building_id_filter: Option<&str>,
    bbox_epsg: Option<BboxUtm>,
) -> Result<(Vec<ParsedBuilding>, Option<DeclaredCrs>), String> {
    let mut buildings = Vec::new();
    let mut declared = Vec::new();
    let mut add = |text: &str| -> Result<(), String> {
        let root = parse_cityjson_json(text)?;
        declared.push(cityjson_declared_crs(&root));
        buildings.extend(parse_cityjson_value(&root, building_id_filter, bbox_epsg)?);
        Ok(())
    };
    match source {
        CityJsonSource::Texts(docs) => {
            for text in docs {
                add(text)?;
            }
        }
        CityJsonSource::Paths(paths) => {
            if paths.is_empty() {
                return Err("cityjson source list is empty".into());
            }
            for path in paths {
                add(&fs::read_to_string(path).map_err(|e| format!("open {path}: {e}"))?)?;
            }
        }
    }
    if buildings.is_empty() {
        return Err("no buildings parsed from CityJSON".into());
    }
    Ok((buildings, common_declared_crs(declared)?))
}

/// Coordinate transformation of x/y in place, called once with all points of
/// all buildings. Z is never passed; heights stay as parsed.
pub type XyTransform<'a> = dyn FnMut(&mut [f64], &mut [f64]) -> Result<(), String> + 'a;

/// Apply `transform` to every ring and interior point of `buildings`.
pub fn transform_buildings(buildings: &mut [ParsedBuilding], transform: &mut XyTransform) -> Result<(), String> {
    let points = || {
        buildings
            .iter()
            .flat_map(|b| &b.boundaries)
            .flat_map(|bd| std::iter::once(&bd.ring).chain(&bd.interiors))
            .flatten()
    };
    let mut xs: Vec<f64> = points().map(|p| p[0]).collect();
    let mut ys: Vec<f64> = points().map(|p| p[1]).collect();
    transform(&mut xs, &mut ys)?;
    if let Some(bad) = xs.iter().zip(&ys).position(|(x, y)| !x.is_finite() || !y.is_finite()) {
        return Err(format!("transform returned a non-finite coordinate for point {bad}"));
    }
    let mut out = xs.into_iter().zip(ys);
    for p in buildings
        .iter_mut()
        .flat_map(|b| &mut b.boundaries)
        .flat_map(|bd| std::iter::once(&mut bd.ring).chain(&mut bd.interiors))
        .flatten()
    {
        (p[0], p[1]) = out.next().ok_or("transform returned fewer points")?;
    }
    Ok(())
}

/// Parse CityGML (one file or many) and write a single IFC.
///
/// `bbox_epsg` filters in the source CRS; `transform` then maps the kept
/// coordinates into the CRS of `opts.crs` / `opts.basepoint`. `opts.crs` is
/// checked against the `srsName` of the files and gets their vertical datum
/// when it has none (see [`reconcile_crs`]).
pub fn gml_to_ifc(
    paths: &[String],
    ifc_path: &str,
    building_id_filter: Option<&str>,
    bbox_epsg: Option<BboxUtm>,
    transform: Option<&mut XyTransform>,
    opts: &ExportOptions,
) -> Result<String, String> {
    let declared = common_declared_crs(paths.iter().map(gml_declared_crs).collect::<Result<Vec<_>, _>>()?)?;
    let crs = reconcile_crs(opts.crs.clone(), declared.as_ref(), transform.is_some())?;
    let t0 = Instant::now();
    let mut buildings = parse_gml_sources(paths, building_id_filter, bbox_epsg, opts.progress)?;
    if opts.progress {
        eprintln!(
            "  parse: {:.2}s ({} buildings)",
            t0.elapsed().as_secs_f64(),
            buildings.len()
        );
    }
    if let Some(transform) = transform {
        transform_buildings(&mut buildings, transform)?;
    }
    write_buildings(&buildings, ifc_path, opts, crs)
}

/// Parse CityJSON (text, one file, or many) and write a single IFC.
/// `bbox_epsg`, `transform` and the `crs` check work as in [`gml_to_ifc`],
/// with `metadata.referenceSystem` as the declared CRS.
pub fn cityjson_to_ifc(
    source: &CityJsonSource,
    ifc_path: &str,
    building_id_filter: Option<&str>,
    bbox_epsg: Option<BboxUtm>,
    transform: Option<&mut XyTransform>,
    opts: &ExportOptions,
) -> Result<String, String> {
    let (mut buildings, declared) = parse_cityjson_source(source, building_id_filter, bbox_epsg)?;
    let crs = reconcile_crs(opts.crs.clone(), declared.as_ref(), transform.is_some())?;
    if let Some(transform) = transform {
        transform_buildings(&mut buildings, transform)?;
    }
    write_buildings(&buildings, ifc_path, opts, crs)
}

/// TreesGenericApp path: Python prepares records, Rust meshes trunk + crown and writes STEP.
///
/// `drape_vertices`: optional DGM points. When set, trees with `z = 0` or nodata
/// take Z from the containing triangle (`drape_faces`) or the nearest vertex.
pub fn trees_to_ifc(
    trees: &[TreeInput],
    ifc_path: &str,
    opts: &ExportOptions,
    drape_vertices: Option<&[[f64; 3]]>,
    drape_faces: Option<&[Face]>,
) -> Result<String, String> {
    if trees.is_empty() {
        return Err("no trees to write".into());
    }
    let mut trees = trees.to_vec();
    if let Some(samples) = drape_vertices {
        let n = drape_trees(&mut trees, samples, drape_faces);
        if opts.progress && n > 0 {
            let how = if drape_faces.is_some() {
                "mesh triangle / nearest vertex"
            } else {
                "nearest sample"
            };
            eprintln!("  trees: draped {n} z=0 / nodata from {how}");
        }
    }
    let mut products = trees_to_products(&trees);
    if !opts.psets.is_empty() {
        for (product, tree) in products.iter_mut().zip(trees.iter()) {
            for pset in psetspec::eval_map(&opts.psets, Scope::Building, &tree_attr_map(tree)) {
                upsert_pset(&mut product.psets, pset);
            }
        }
    }
    let mut write = opts.write_options();
    write.city_model = true;
    write_ifc_file(&group_products(products), ifc_path, &write)?;
    Ok(ifc_path.to_string())
}

/// Local origin all terrain vertices are shifted by: the basepoint if given,
/// else the lower-left corner of `vertices`.
fn terrain_origin<'a>(
    vertices: impl IntoIterator<Item = &'a [f64; 3]>,
    opts: &ExportOptions,
) -> [f64; 3] {
    match opts.basepoint {
        Some((x, y)) => [x, y, 0.0],
        None => {
            let (min_x, min_y) = vertices.into_iter().fold(
                (f64::INFINITY, f64::INFINITY),
                |(ax, ay), v| (ax.min(v[0]), ay.min(v[1])),
            );
            [min_x, min_y, 0.0]
        }
    }
}

/// One `TerrainInput` → one styled `Product` (nodata fill + spec psets).
fn terrain_product(mesh: &TerrainInput, opts: &ExportOptions) -> Result<Product, String> {
    let n_bad = mesh.vertices.iter().filter(|v| !elevation_ok(v[2])).count();
    let mut product = terrain_to_product(mesh)?;
    if opts.progress && n_bad > 0 {
        eprintln!("  terrain {}: filled {n_bad} nodata z from nearest valid", mesh.name);
    }
    if !opts.psets.is_empty() {
        for pset in psetspec::eval_map(&opts.psets, Scope::Building, &terrain_attr_map(mesh)) {
            upsert_pset(&mut product.psets, pset);
        }
    }
    Ok(product)
}

/// Terrain products are proxies contained directly in the site, never wrapped
/// in an `IfcBuilding` and never a city model.
fn write_terrain(products: Vec<Product>, ifc_path: &str, opts: &ExportOptions) -> Result<String, String> {
    let mut write = opts.write_options();
    write.city_model = false;
    write.contain_in_site = true;
    write_ifc_file(&group_products(products), ifc_path, &write)?;
    Ok(ifc_path.to_string())
}

/// TerrainGenericApp write path: Python still meshes; Rust writes STEP.
pub fn terrain_to_ifc(mesh: &TerrainInput, ifc_path: &str, opts: &ExportOptions) -> Result<String, String> {
    let mut mesh = mesh.clone();
    if mesh.placement.is_none() {
        mesh.placement = Some(terrain_origin(mesh.vertices.iter(), opts));
    }
    let product = terrain_product(&mesh, opts)?;
    write_terrain(vec![product], ifc_path, opts)
}

/// Guided DGM write path: many land-use parts (Fahrbahn, water, Parcels, …)
/// into one IFC, each its own styled proxy. All parts share one local origin
/// so they line up. Each part carries its own concrete psets and color/layer.
pub fn terrain_parts_to_ifc(
    parts: &[TerrainInput],
    ifc_path: &str,
    opts: &ExportOptions,
) -> Result<String, String> {
    if parts.is_empty() {
        return Err("no terrain parts to write".into());
    }
    let origin = terrain_origin(parts.iter().flat_map(|m| m.vertices.iter()), opts);
    let mut products = Vec::with_capacity(parts.len());
    for part in parts {
        let mut mesh = part.clone();
        if mesh.placement.is_none() {
            mesh.placement = Some(origin);
        }
        products.push(terrain_product(&mesh, opts)?);
    }
    write_terrain(products, ifc_path, opts)
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
    crs: Option<ProjectedCrs>,
) -> Result<String, String> {
    let t0 = Instant::now();
    let export = buildings_to_export(buildings, &opts.export_config());
    if opts.progress {
        eprintln!(
            "  export: {:.2}s ({} buildings)",
            t0.elapsed().as_secs_f64(),
            export.len()
        );
    }
    let t1 = Instant::now();
    let mut write = opts.write_options();
    write.crs = crs;
    write_ifc_file(&export, ifc_path, &write)?;
    if opts.progress {
        eprintln!("  write: {:.2}s", t1.elapsed().as_secs_f64());
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Boundary, BuildingAttributes};

    fn building() -> ParsedBuilding {
        ParsedBuilding {
            id: "b1".into(),
            gml_name: None,
            lod: "2".into(),
            attributes: BuildingAttributes::default(),
            boundaries: vec![Boundary {
                ring: vec![[0.0, 0.0, 5.0], [10.0, 0.0, 5.0], [10.0, 10.0, 7.0]],
                interiors: vec![vec![[2.0, 2.0, 6.0]]],
                surface_type: "WallSurface".into(),
                source_part_id: None,
            }],
        }
    }

    #[test]
    fn transform_buildings_moves_ring_and_interior_xy_keeps_z() {
        let mut buildings = vec![building()];
        let mut calls = 0;
        let mut shift = |xs: &mut [f64], ys: &mut [f64]| {
            calls += 1;
            xs.iter_mut().for_each(|x| *x += 100.0);
            ys.iter_mut().for_each(|y| *y += 200.0);
            Ok(())
        };
        transform_buildings(&mut buildings, &mut shift).unwrap();
        assert_eq!(calls, 1);
        let bd = &buildings[0].boundaries[0];
        assert_eq!(bd.ring, vec![[100.0, 200.0, 5.0], [110.0, 200.0, 5.0], [110.0, 210.0, 7.0]]);
        assert_eq!(bd.interiors, vec![vec![[102.0, 202.0, 6.0]]]);
    }

    #[test]
    fn transform_buildings_rejects_non_finite_result() {
        let mut buildings = vec![building()];
        let mut broken = |xs: &mut [f64], _: &mut [f64]| {
            xs[1] = f64::INFINITY;
            Ok(())
        };
        let err = transform_buildings(&mut buildings, &mut broken).unwrap_err();
        assert!(err.contains("non-finite"), "{err}");
    }
}
