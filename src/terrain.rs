//! One terrain mesh → one `IfcBuildingElementProxy`. Same input as TerrainGenericApp.

use std::collections::HashMap;

use crate::color::TERRAIN_RGB;
use crate::types::{Face, Product};

pub const TERRAIN_LAYER: &str = "_BIM_DGM_Gelaende";

/// NHN elevations in Hamburg are tens of metres. GeoTIFF nodata is `f32::MIN`
/// (`≈ -3.4e38`), not water. Water in DGM1 is usually that sentinel, not `z ≈ 0`.
const MAX_ABS_ELEVATION_M: f64 = 10_000.0;

pub(crate) fn elevation_ok(z: f64) -> bool {
    z.is_finite() && z.abs() < MAX_ABS_ELEVATION_M
}

/// Keep XY; replace nodata / non-finite Z with the nearest valid vertex Z (XY).
/// Not a fixed `0` and not the crop minimum.
fn fill_nodata_from_nearest(vertices: &[[f64; 3]]) -> Result<(Vec<[f64; 3]>, usize), String> {
    let valid: Vec<usize> = vertices
        .iter()
        .enumerate()
        .filter(|(_, v)| elevation_ok(v[2]))
        .map(|(i, _)| i)
        .collect();
    if valid.is_empty() {
        return Err("terrain mesh has no valid elevations".into());
    }
    if valid.len() == vertices.len() {
        return Ok((vertices.to_vec(), 0));
    }
    let mut out = vertices.to_vec();
    let mut filled = 0usize;
    for (i, v) in vertices.iter().enumerate() {
        if elevation_ok(v[2]) {
            continue;
        }
        let mut best_d = f64::INFINITY;
        let mut best_z = 0.0;
        for &j in &valid {
            let dx = vertices[j][0] - v[0];
            let dy = vertices[j][1] - v[1];
            let d = dx * dx + dy * dy;
            if d < best_d {
                best_d = d;
                best_z = vertices[j][2];
            }
        }
        out[i][2] = best_z;
        filled += 1;
    }
    Ok((out, filled))
}

/// Mesh Python / BIMFabrik already triangulated (`TerrainMesh`).
#[derive(Clone, Debug)]
pub struct TerrainInput {
    pub name: String,
    pub vertices: Vec<[f64; 3]>,
    pub faces: Vec<Face>,
    pub color: [f64; 3],
    pub layer: Option<String>,
    pub attributes: HashMap<String, String>,
    /// World origin. Vertices are shifted into local coords (z kept). Same as trees.
    pub placement: Option<[f64; 3]>,
}

impl Default for TerrainInput {
    fn default() -> Self {
        Self {
            name: "DGM".into(),
            vertices: Vec::new(),
            faces: Vec::new(),
            color: TERRAIN_RGB,
            layer: Some(TERRAIN_LAYER.into()),
            attributes: HashMap::new(),
            placement: None,
        }
    }
}

pub fn terrain_to_product(mesh: &TerrainInput) -> Result<Product, String> {
    if mesh.vertices.len() < 3 {
        return Err("terrain mesh needs at least 3 vertices".into());
    }
    if mesh.faces.is_empty() {
        return Err("terrain mesh has no faces".into());
    }
    let n = mesh.vertices.len() as u32;
    for face in &mesh.faces {
        let idx = match face {
            Face::Simple(idx) => idx.as_slice(),
            Face::WithVoids { outer, .. } => outer.as_slice(),
        };
        if idx.len() < 3 {
            return Err("terrain face needs at least 3 indices".into());
        }
        if idx.iter().any(|&i| i >= n) {
            return Err("terrain face index out of range".into());
        }
    }
    let (vertices, _filled) = fill_nodata_from_nearest(&mesh.vertices)?;
    let origin = mesh.placement.unwrap_or([0.0, 0.0, 0.0]);
    let vertices = vertices
        .iter()
        .map(|v| [v[0] - origin[0], v[1] - origin[1], v[2] - origin[2]])
        .collect();
    Ok(Product {
        name: mesh.name.clone(),
        ifc_type: "IfcBuildingElementProxy".into(),
        parent: mesh.name.clone(),
        vertices,
        faces: mesh.faces.clone(),
        psets: Vec::new(),
        color: Some(mesh.color),
        layer: mesh.layer.clone(),
        extra: Vec::new(),
        placement: mesh.placement,
    })
}

pub fn terrain_attr_map(mesh: &TerrainInput) -> HashMap<String, String> {
    let mut attrs = mesh.attributes.clone();
    attrs.entry("name".into()).or_insert_with(|| mesh.name.clone());
    attrs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_triangles_become_one_proxy() {
        let mesh = TerrainInput {
            vertices: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.1],
                [0.0, 1.0, 0.0],
            ],
            faces: vec![Face::Simple(vec![0, 1, 2]), Face::Simple(vec![0, 2, 3])],
            ..TerrainInput::default()
        };
        let product = terrain_to_product(&mesh).unwrap();
        assert_eq!(product.vertices.len(), 4);
        assert_eq!(product.faces.len(), 2);
        assert_eq!(product.layer.as_deref(), Some(TERRAIN_LAYER));
        assert_eq!(product.ifc_type, "IfcBuildingElementProxy");
        assert_eq!(product.placement, None);
        assert_eq!(product.vertices[0], [0.0, 0.0, 0.0]);
    }

    #[test]
    fn placement_shifts_xy_like_trees() {
        let mesh = TerrainInput {
            vertices: vec![
                [565000.0, 5933000.0, 4.0],
                [565001.0, 5933000.0, 4.0],
                [565001.0, 5933001.0, 5.0],
            ],
            faces: vec![Face::Simple(vec![0, 1, 2])],
            placement: Some([565000.0, 5933000.0, 0.0]),
            ..TerrainInput::default()
        };
        let product = terrain_to_product(&mesh).unwrap();
        assert_eq!(product.placement, Some([565000.0, 5933000.0, 0.0]));
        assert_eq!(product.vertices[0], [0.0, 0.0, 4.0]);
        assert_eq!(product.vertices[2], [1.0, 1.0, 5.0]);
    }

    #[test]
    fn rejects_empty_mesh() {
        assert!(terrain_to_product(&TerrainInput::default()).is_err());
    }

    #[test]
    fn fills_nodata_z_from_nearest_valid_not_zero() {
        let nodata = f64::from(f32::MIN);
        let mesh = TerrainInput {
            vertices: vec![
                [0.0, 0.0, 1.5],
                [10.0, 0.0, nodata],
                [100.0, 0.0, 9.0],
                [0.0, 10.0, 4.0],
            ],
            faces: vec![Face::Simple(vec![0, 1, 2]), Face::Simple(vec![0, 2, 3])],
            ..TerrainInput::default()
        };
        let product = terrain_to_product(&mesh).unwrap();
        assert_eq!(product.vertices.len(), 4);
        assert_eq!(product.faces.len(), 2);
        assert_eq!(product.vertices[1], [10.0, 0.0, 1.5]);
        assert!(product.vertices.iter().all(|v| elevation_ok(v[2])));
    }
}
