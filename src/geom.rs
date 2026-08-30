//! Quantities and building → IFC product conversion.

use std::collections::HashMap;

use crate::color::Colors;
use crate::psetspec::{self, Needs, PsetSpec, Scope, SurfaceFacts};
use crate::types::{
    building_display_name, lookup_ifc_type, strip_close, upsert_pset, Boundary,
    BuildingExport, Face, ParsedBuilding, Product,
};

pub fn polygon_gross_area(ring: &[[f64; 3]]) -> f64 {
    let pts = open_ring(ring);
    if pts.len() < 3 {
        return 0.0;
    }
    let v0 = pts[0];
    let mut cx = 0.0;
    let mut cy = 0.0;
    let mut cz = 0.0;
    for i in 1..pts.len() - 1 {
        let a = sub(pts[i], v0);
        let b = sub(pts[i + 1], v0);
        let c = cross(a, b);
        cx += c[0];
        cy += c[1];
        cz += c[2];
    }
    0.5 * (cx * cx + cy * cy + cz * cz).sqrt()
}

pub fn polygon_perimeter(ring: &[[f64; 3]]) -> f64 {
    let pts = open_ring(ring);
    if pts.len() < 2 {
        return 0.0;
    }
    let mut sum = 0.0;
    for i in 0..pts.len() {
        let j = (i + 1) % pts.len();
        sum += dist(pts[i], pts[j]);
    }
    sum
}

pub fn polygon_slope_deg(ring: &[[f64; 3]]) -> f64 {
    let pts = open_ring(ring);
    if pts.len() < 3 {
        return 0.0;
    }
    let v0 = pts[0];
    let mut normal = [0.0, 0.0, 0.0];
    for i in 1..pts.len() - 1 {
        let c = cross(sub(pts[i], v0), sub(pts[i + 1], v0));
        if norm(c) > 1e-9 {
            normal = c;
            break;
        }
    }
    let n = norm(normal);
    if n < 1e-9 {
        return 0.0;
    }
    let cos_theta = (normal[2].abs() / n).clamp(0.0, 1.0);
    cos_theta.acos().to_degrees()
}

fn open_ring(ring: &[[f64; 3]]) -> Vec<[f64; 3]> {
    let mut out = ring.to_vec();
    strip_close(&mut out);
    out
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(a: [f64; 3]) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}
fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    norm(sub(a, b))
}

fn face_from_boundary(b: &Boundary) -> (Vec<[f64; 3]>, Face) {
    let mut vertices = b.ring.clone();
    let outer: Vec<u32> = (0..b.ring.len() as u32).collect();
    let mut inners = Vec::new();
    let mut offset = b.ring.len() as u32;
    for inner in &b.interiors {
        let n = inner.len() as u32;
        inners.push((offset..offset + n).collect());
        vertices.extend_from_slice(inner);
        offset += n;
    }
    let face = if inners.is_empty() {
        Face::Simple(outer)
    } else {
        Face::WithVoids { outer, inners }
    };
    (vertices, face)
}

/// How buildings turn into IFC products, including where psets come from.
pub struct ExportConfig<'a> {
    pub mode: &'a str,
    pub styles: bool,
    pub colors: &'a Colors,
    /// Empty means no property sets are written.
    pub specs: &'a [PsetSpec],
    /// CityGML/CityJSON surface type → IFC entity. Missing keys (and an empty
    /// map) become `IfcBuildingElementProxy`. Mesh mode reads the `"Building"` key.
    pub ifc_types: &'a HashMap<String, String>,
}

impl ExportConfig<'_> {
    fn typed(&self) -> bool {
        self.mode.eq_ignore_ascii_case("typed")
    }
}

pub fn buildings_to_export(buildings: &[ParsedBuilding], cfg: &ExportConfig<'_>) -> Vec<BuildingExport> {
    let specs = cfg.specs;
    let needs = Needs::of(specs);
    buildings
        .iter()
        .filter_map(|b| {
            let products = if cfg.typed() {
                typed_products(b, cfg, specs, needs)
            } else {
                mesh_product(b, cfg, specs, needs).into_iter().collect()
            };
            if products.is_empty() {
                return None;
            }
            Some(BuildingExport {
                id: b.id.clone(),
                display_name: building_display_name(b),
                psets: if cfg.typed() {
                    psetspec::eval(specs, Scope::Building, b, None)
                } else {
                    Vec::new()
                },
                products,
            })
        })
        .collect()
}

fn typed_products(
    b: &ParsedBuilding,
    cfg: &ExportConfig<'_>,
    specs: &[PsetSpec],
    needs: Needs,
) -> Vec<Product> {
    b.boundaries
        .iter()
        .enumerate()
        .map(|(idx, bd)| {
            let (vertices, face) = face_from_boundary(bd);
            let mut name_parts = vec![building_display_name(b), bd.surface_type.clone(), idx.to_string()];
            if let Some(part) = &bd.source_part_id {
                name_parts.insert(1, part.clone());
            }
            let facts = SurfaceFacts {
                area: if needs.area { polygon_gross_area(&bd.ring) } else { 0.0 },
                perimeter: if needs.perimeter { polygon_perimeter(&bd.ring) } else { 0.0 },
                tilt: if needs.tilt { polygon_slope_deg(&bd.ring) } else { 0.0 },
                surface_type: &bd.surface_type,
            };
            let psets = psetspec::eval(specs, Scope::Surface, b, Some(&facts));
            Product {
                name: name_parts.join("_"),
                ifc_type: lookup_ifc_type(cfg.ifc_types, &bd.surface_type).to_string(),
                parent: b.id.clone(),
                vertices,
                faces: vec![face],
                psets,
                color: if cfg.styles {
                    Some(cfg.colors.rgb(&bd.surface_type))
                } else {
                    None
                },
                layer: None,
                extra: Vec::new(),
                placement: None,
            }
        })
        .collect()
}

fn mesh_product(
    b: &ParsedBuilding,
    cfg: &ExportConfig<'_>,
    specs: &[PsetSpec],
    needs: Needs,
) -> Option<Product> {
    let mut vertices: Vec<[f64; 3]> = Vec::new();
    let mut faces = Vec::new();
    let mut index_of: HashMap<(i64, i64, i64), u32> = HashMap::new();
    let key = |p: [f64; 3]| {
        (
            (p[0] * 1_000.0).round() as i64,
            (p[1] * 1_000.0).round() as i64,
            (p[2] * 1_000.0).round() as i64,
        )
    };
    let mut intern = |p: [f64; 3], vertices: &mut Vec<[f64; 3]>| {
        let k = key(p);
        if let Some(&i) = index_of.get(&k) {
            return i;
        }
        let i = vertices.len() as u32;
        index_of.insert(k, i);
        vertices.push(p);
        i
    };
    let mut area = 0.0;
    let mut peri = 0.0;
    for bd in &b.boundaries {
        if needs.area {
            area += polygon_gross_area(&bd.ring);
        }
        if needs.perimeter {
            peri += polygon_perimeter(&bd.ring);
        }
        let outer: Vec<u32> = bd.ring.iter().map(|p| intern(*p, &mut vertices)).collect();
        if outer.len() < 3 {
            continue;
        }
        let inners: Vec<Vec<u32>> = bd
            .interiors
            .iter()
            .map(|ring| ring.iter().map(|p| intern(*p, &mut vertices)).collect())
            .filter(|ring: &Vec<u32>| ring.len() >= 3)
            .collect();
        faces.push(if inners.is_empty() {
            Face::Simple(outer)
        } else {
            Face::WithVoids { outer, inners }
        });
    }
    if faces.is_empty() {
        return None;
    }
    // This one product stands for the whole building, so it carries both scopes:
    // surface-scope specs see the building-wide aggregates.
    let facts = SurfaceFacts { area, perimeter: peri, tilt: 0.0, surface_type: &b.lod };
    let mut psets = psetspec::eval(specs, Scope::Building, b, None);
    for pset in psetspec::eval(specs, Scope::Surface, b, Some(&facts)) {
        upsert_pset(&mut psets, pset);
    }
    Some(Product {
        name: building_display_name(b),
        ifc_type: lookup_ifc_type(cfg.ifc_types, "Building").to_string(),
        parent: b.id.clone(),
        vertices,
        faces,
        psets,
        color: if cfg.styles {
            Some(cfg.colors.rgb("Building"))
        } else {
            None
        },
        layer: None,
        extra: Vec::new(),
        placement: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_dresden_wall_quantities() {
        let ring = [
            [410435.066, 5653805.371, 162.021],
            [410425.219, 5653803.967, 161.867],
            [410425.219, 5653803.967, 141.169],
            [410435.066, 5653805.371, 141.169],
        ];
        assert!((polygon_gross_area(&ring) - 206.6404).abs() < 1e-3);
        assert!((polygon_perimeter(&ring) - 61.4444).abs() < 1e-3);
        assert!((polygon_slope_deg(&ring) - 90.0).abs() < 1e-6);
    }
}
