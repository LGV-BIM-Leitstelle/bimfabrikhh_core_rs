//! Trunk (n-gon cylinder) + crown (icosphere). Same meshes as TreesGenericApp / ifcfactory.

use std::collections::HashMap;

use crate::color::{CROWN_RGB, TRUNK_RGB};
use crate::terrain::{elevation_ok, nearest_valid_z};
use crate::types::{Face, Product, ProductPart, Pset};

/// Barycentric Z if ``(x, y)`` lies in triangle ``a,b,c`` (XY).
fn triangle_z(x: f64, y: f64, a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> Option<f64> {
    if !elevation_ok(a[2]) || !elevation_ok(b[2]) || !elevation_ok(c[2]) {
        return None;
    }
    let ax = b[0] - a[0];
    let ay = b[1] - a[1];
    let bx = c[0] - a[0];
    let by = c[1] - a[1];
    let cx = x - a[0];
    let cy = y - a[1];
    let den = ax * by - ay * bx;
    if den.abs() < 1e-18 {
        return None;
    }
    let u = (cx * by - cy * bx) / den;
    let v = (ax * cy - ay * cx) / den;
    if u >= -1e-8 && v >= -1e-8 && u + v <= 1.0 + 1e-8 {
        Some(a[2] + u * (b[2] - a[2]) + v * (c[2] - a[2]))
    } else {
        None
    }
}

fn mesh_z_at(x: f64, y: f64, verts: &[[f64; 3]], faces: &[Face]) -> Option<f64> {
    for face in faces {
        let idx = match face {
            Face::Simple(idx) if idx.len() >= 3 => idx.as_slice(),
            Face::WithVoids { outer, .. } if outer.len() >= 3 => outer.as_slice(),
            _ => continue,
        };
        let (i0, i1, i2) = (idx[0] as usize, idx[1] as usize, idx[2] as usize);
        if i0 >= verts.len() || i1 >= verts.len() || i2 >= verts.len() {
            continue;
        }
        if let Some(z) = triangle_z(x, y, verts[i0], verts[i1], verts[i2]) {
            return Some(z);
        }
    }
    None
}

pub const TRUNK_LAYER: &str = "_BIM_SBK_Stamm";
pub const CROWN_LAYER: &str = "_BIM_SBK_Krone";

/// One tree as Python / intern pass it after `resolve_tree_dimensions`.
#[derive(Clone, Debug)]
pub struct TreeInput {
    pub name: String,
    pub position: [f64; 3],
    pub trunk_radius: f64,
    pub trunk_height: f64,
    pub trunk_segments: u32,
    pub crown_radius: f64,
    pub crown_detail: u32,
    pub include_crown: bool,
    pub psets: Vec<Pset>,
    /// Source fields for [`crate::psetspec`] (same idea as CityGML `extra`).
    pub attributes: HashMap<String, String>,
    pub trunk_color: [f64; 3],
    pub crown_color: [f64; 3],
}

impl Default for TreeInput {
    fn default() -> Self {
        Self {
            name: "Baum".into(),
            position: [0.0, 0.0, 0.0],
            trunk_radius: 0.3,
            trunk_height: 4.0,
            trunk_segments: 8,
            crown_radius: 2.5,
            crown_detail: 1,
            include_crown: true,
            psets: Vec::new(),
            attributes: HashMap::new(),
            trunk_color: TRUNK_RGB,
            crown_color: CROWN_RGB,
        }
    }
}

pub fn trees_to_products(trees: &[TreeInput]) -> Vec<Product> {
    trees.iter().map(tree_to_product).collect()
}

/// BIMFabrik sampler writes nodata as `0`. Optional rust drape treats that as missing.
pub fn tree_z_needs_drape(z: f64) -> bool {
    z == 0.0 || !elevation_ok(z)
}

/// Set `position[2]` from the mesh: containing-triangle Z, else nearest vertex.
/// No-op when `samples` is empty.
pub fn drape_trees(trees: &mut [TreeInput], samples: &[[f64; 3]], faces: Option<&[Face]>) -> usize {
    if samples.is_empty() {
        return 0;
    }
    let mut filled = 0usize;
    for tree in trees {
        if !tree_z_needs_drape(tree.position[2]) {
            continue;
        }
        let z = faces
            .and_then(|f| mesh_z_at(tree.position[0], tree.position[1], samples, f))
            .or_else(|| nearest_valid_z(tree.position[0], tree.position[1], samples));
        if let Some(z) = z {
            tree.position[2] = z;
            filled += 1;
        }
    }
    filled
}

/// Geometry plus caller attributes. Existing keys win over the derived ones.
pub fn tree_attr_map(tree: &TreeInput) -> HashMap<String, String> {
    let mut attrs = tree.attributes.clone();
    attrs.entry("name".into()).or_insert_with(|| tree.name.clone());
    attrs
        .entry("trunk_radius".into())
        .or_insert_with(|| tree.trunk_radius.to_string());
    attrs
        .entry("trunk_height".into())
        .or_insert_with(|| tree.trunk_height.to_string());
    attrs
        .entry("crown_radius".into())
        .or_insert_with(|| tree.crown_radius.to_string());
    attrs
        .entry("stammdurchmesser".into())
        .or_insert_with(|| (tree.trunk_radius * 2.0).to_string());
    attrs
        .entry("kronendurchmesser".into())
        .or_insert_with(|| (tree.crown_radius * 2.0).to_string());
    attrs
}

fn tree_to_product(tree: &TreeInput) -> Product {
    let segs = tree.trunk_segments.max(3);
    let (trunk_v, trunk_f) = ngon_cylinder(tree.trunk_radius, tree.trunk_height, segs);

    let mut extra = Vec::new();
    if tree.include_crown && tree.crown_radius > 0.0 {
        let detail = tree.crown_detail.max(1);
        let (mut crown_v, crown_tri) = icosphere(tree.crown_radius, detail);
        translate(&mut crown_v, [0.0, 0.0, tree.trunk_height]);
        extra.push(ProductPart {
            vertices: crown_v,
            faces: crown_tri
                .into_iter()
                .map(|[a, b, c]| Face::Simple(vec![a, b, c]))
                .collect(),
            color: Some(tree.crown_color),
            layer: Some(CROWN_LAYER.into()),
        });
    }

    Product {
        name: tree.name.clone(),
        ifc_type: "IfcBuildingElementProxy".into(),
        parent: "Trees".into(),
        vertices: trunk_v,
        faces: trunk_f,
        psets: tree.psets.clone(),
        color: Some(tree.trunk_color),
        layer: Some(TRUNK_LAYER.into()),
        extra,
        placement: Some(tree.position),
    }
}

fn translate(verts: &mut [[f64; 3]], d: [f64; 3]) {
    for v in verts {
        v[0] += d[0];
        v[1] += d[1];
        v[2] += d[2];
    }
}

/// Unit-frequency geodesic icosahedron, then scaled. `detail` is ifcfactory `Sphere.detail` / `nu`.
pub fn icosphere(radius: f64, detail: u32) -> (Vec<[f64; 3]>, Vec<[u32; 3]>) {
    let (verts, faces) = regular_icosahedron();
    let (mut verts, faces) = subdivide_frequency(verts, faces, detail.max(1));
    for v in &mut verts {
        *v = [v[0] * radius, v[1] * radius, v[2] * radius];
    }
    (verts, faces)
}

fn regular_icosahedron() -> (Vec<[f64; 3]>, Vec<[u32; 3]>) {
    let t = (1.0 + 5.0_f64.sqrt()) / 2.0;
    let raw = [
        [-1.0, t, 0.0],
        [1.0, t, 0.0],
        [-1.0, -t, 0.0],
        [1.0, -t, 0.0],
        [0.0, -1.0, t],
        [0.0, 1.0, t],
        [0.0, -1.0, -t],
        [0.0, 1.0, -t],
        [t, 0.0, -1.0],
        [t, 0.0, 1.0],
        [-t, 0.0, -1.0],
        [-t, 0.0, 1.0],
    ];
    let verts = raw.into_iter().map(normalize).collect();
    let faces = vec![
        [0, 11, 5],
        [0, 5, 1],
        [0, 1, 7],
        [0, 7, 10],
        [0, 10, 11],
        [1, 5, 9],
        [5, 11, 4],
        [11, 10, 2],
        [10, 7, 6],
        [7, 1, 8],
        [3, 9, 4],
        [3, 4, 2],
        [3, 2, 6],
        [3, 6, 8],
        [3, 8, 9],
        [4, 9, 5],
        [2, 4, 11],
        [6, 2, 10],
        [8, 6, 7],
        [9, 8, 1],
    ];
    (verts, faces)
}

/// ifcfactory `icosphere(nu)`: each edge into `nu` segments, each face into `nu²` triangles.
fn subdivide_frequency(
    verts: Vec<[f64; 3]>,
    faces: Vec<[u32; 3]>,
    nu: u32,
) -> (Vec<[f64; 3]>, Vec<[u32; 3]>) {
    if nu <= 1 {
        return (verts, faces);
    }
    let mut out_v = Vec::new();
    let mut index: HashMap<(i64, i64, i64), u32> = HashMap::new();
    let mut intern = |p: [f64; 3], out_v: &mut Vec<[f64; 3]>| {
        let n = normalize(p);
        let k = (
            (n[0] * 1e9).round() as i64,
            (n[1] * 1e9).round() as i64,
            (n[2] * 1e9).round() as i64,
        );
        if let Some(&i) = index.get(&k) {
            return i;
        }
        let i = out_v.len() as u32;
        index.insert(k, i);
        out_v.push(n);
        i
    };

    let mut out_f = Vec::new();
    let nu = nu as usize;
    for [a, b, c] in faces {
        let va = verts[a as usize];
        let vb = verts[b as usize];
        let vc = verts[c as usize];
        let mut grid = vec![vec![0u32; nu + 1]; nu + 1];
        for i in 0..=nu {
            for j in 0..=(nu - i) {
                let k = nu - i - j;
                let s = nu as f64;
                let p = [
                    (k as f64 * va[0] + i as f64 * vb[0] + j as f64 * vc[0]) / s,
                    (k as f64 * va[1] + i as f64 * vb[1] + j as f64 * vc[1]) / s,
                    (k as f64 * va[2] + i as f64 * vb[2] + j as f64 * vc[2]) / s,
                ];
                grid[i][j] = intern(p, &mut out_v);
            }
        }
        for i in 0..nu {
            for j in 0..(nu - i) {
                let p00 = grid[i][j];
                let p10 = grid[i + 1][j];
                let p01 = grid[i][j + 1];
                out_f.push([p00, p10, p01]);
                if i + j + 2 <= nu {
                    let p11 = grid[i + 1][j + 1];
                    out_f.push([p10, p11, p01]);
                }
            }
        }
    }
    (out_v, out_f)
}

fn normalize(v: [f64; 3]) -> [f64; 3] {
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if n < 1e-18 {
        return v;
    }
    [v[0] / n, v[1] / n, v[2] / n]
}

/// Prism along +Z, base on z=0. Same idea as ifcfactory `NgonCylinder`.
pub fn ngon_cylinder(radius: f64, height: f64, segments: u32) -> (Vec<[f64; 3]>, Vec<Face>) {
    let n = segments.max(3) as usize;
    let mut verts = Vec::with_capacity(2 * n);
    for i in 0..n {
        let a = std::f64::consts::TAU * (i as f64) / (n as f64);
        let x = radius * a.cos();
        let y = radius * a.sin();
        verts.push([x, y, 0.0]);
    }
    for i in 0..n {
        let a = std::f64::consts::TAU * (i as f64) / (n as f64);
        verts.push([radius * a.cos(), radius * a.sin(), height]);
    }
    let mut faces = Vec::with_capacity(n + 2);
    for i in 0..n {
        let j = (i + 1) % n;
        let b0 = i as u32;
        let b1 = j as u32;
        let t0 = (n + i) as u32;
        let t1 = (n + j) as u32;
        faces.push(Face::Simple(vec![b0, b1, t1, t0]));
    }
    let bottom: Vec<u32> = (0..n as u32).rev().collect();
    let top: Vec<u32> = (n as u32..(2 * n as u32)).collect();
    faces.push(Face::Simple(bottom));
    faces.push(Face::Simple(top));
    (verts, faces)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icosphere_counts_match_ifcfactory() {
        for nu in 1..=4 {
            let (v, f) = icosphere(1.0, nu);
            let want_v = 12 + 10 * (nu * nu - 1);
            let want_f = 20 * nu * nu;
            assert_eq!(v.len() as u32, want_v, "verts nu={nu}");
            assert_eq!(f.len() as u32, want_f, "faces nu={nu}");
        }
    }

    #[test]
    fn icosphere_vertices_lie_on_sphere() {
        let (v, _) = icosphere(2.5, 3);
        for p in v {
            let r = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            assert!((r - 2.5).abs() < 1e-9);
        }
    }

    #[test]
    fn cylinder_has_sides_and_caps() {
        let (v, f) = ngon_cylinder(0.3, 4.0, 8);
        assert_eq!(v.len(), 16);
        assert_eq!(f.len(), 10);
    }

    #[test]
    fn stump_has_no_crown_part() {
        let mut t = TreeInput::default();
        t.include_crown = false;
        let p = tree_to_product(&t);
        assert!(p.extra.is_empty());
    }

    #[test]
    fn geometry_is_local_placement_holds_world() {
        let mut t = TreeInput::default();
        t.position = [565100.0, 5933600.0, 5.0];
        t.trunk_height = 4.0;
        t.crown_radius = 2.0;
        let p = tree_to_product(&t);
        assert_eq!(p.placement, Some([565100.0, 5933600.0, 5.0]));
        for v in &p.vertices {
            assert!(v[0].abs() < 1.0 && v[1].abs() < 1.0);
            assert!(v[2] >= -1e-9 && v[2] <= 4.0 + 1e-9);
        }
        let crown_z: Vec<f64> = p.extra[0].vertices.iter().map(|v| v[2]).collect();
        assert!(crown_z.iter().all(|z| (z - 4.0).abs() <= 2.0 + 1e-6));
    }

    #[test]
    fn drape_fills_zero_from_nearest_sample() {
        let mut trees = vec![
            TreeInput {
                name: "a".into(),
                position: [10.0, 0.0, 0.0],
                ..TreeInput::default()
            },
            TreeInput {
                name: "b".into(),
                position: [0.0, 0.0, 4.2],
                ..TreeInput::default()
            },
        ];
        let n = drape_trees(&mut trees, &[[0.0, 0.0, 4.2], [100.0, 0.0, 9.0]], None);
        assert_eq!(n, 1);
        assert_eq!(trees[0].position[2], 4.2);
        assert_eq!(trees[1].position[2], 4.2);
    }

    #[test]
    fn drape_skipped_when_no_samples() {
        let mut trees = vec![TreeInput {
            position: [1.0, 2.0, 0.0],
            ..TreeInput::default()
        }];
        assert_eq!(drape_trees(&mut trees, &[], None), 0);
        assert_eq!(trees[0].position[2], 0.0);
    }

    #[test]
    fn drape_uses_triangle_not_far_vertex() {
        let verts = [
            [0.0, 0.0, 1.0],
            [10.0, 0.0, 1.0],
            [0.0, 10.0, 1.0],
            [100.0, 100.0, 40.0],
        ];
        let faces = [Face::Simple(vec![0, 1, 2])];
        let mut trees = vec![TreeInput {
            position: [2.0, 2.0, 0.0],
            ..TreeInput::default()
        }];
        assert_eq!(drape_trees(&mut trees, &verts, Some(&faces)), 1);
        assert!((trees[0].position[2] - 1.0).abs() < 1e-9);
    }
}
