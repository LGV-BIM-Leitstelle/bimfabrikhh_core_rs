//! Soil cylinders and an optional groundwater square. Same solids as BoreholesGenericApp.
//!
//! Python still parses BoreholeML and resolves the code lists. Each solid arrives
//! with its position, size, colour, transparency and concrete property sets.

use std::collections::HashMap;

use crate::tree::ngon_cylinder;
use crate::types::{BuildingExport, Face, Product, Pset};

/// Property sets that describe the whole borehole. Copied onto the assembly
/// and also kept on each part, so a selected layer still shows them.
const ASSEMBLY_PSET_NAMES: &[&str] = &["Pset_Aufschluss", "Pset_Hyperlink"];

/// CAD layer prefix for a soil cylinder. Callers append the DIN colour name.
pub const SOIL_LAYER: &str = "_Bodenaufschluesse";
/// CAD layer of the groundwater square. Same prefix as BoreholesGenericApp.
pub const WATER_LAYER: &str = "_Grundwasser";

/// Groundwater square. Soil cylinders stay opaque (`0`).
pub const GROUNDWATER_TRANSPARENCY: f64 = 0.8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SolidKind {
    /// Vertical cylinder. Base on `position`, height along +Z.
    Cylinder,
    /// Horizontal square on `position`, edge length `size`.
    Square,
}

/// One IFC proxy: a soil layer or the groundwater marker of one borehole.
#[derive(Clone, Debug)]
pub struct BoreholeSolid {
    /// Group key. Solids with the same value become one `IfcElementAssembly`.
    /// Empty keeps each solid in its own assembly.
    pub borehole: String,
    /// Assembly name. Empty uses [`Self::borehole`].
    pub borehole_name: String,
    pub name: String,
    pub kind: SolidKind,
    pub position: [f64; 3],
    /// Cylinder height in metres. Ignored for a square.
    pub height: f64,
    /// Square edge length in metres. Ignored for a cylinder.
    pub size: f64,
    pub radius: f64,
    pub segments: u32,
    pub color: [f64; 3],
    /// 0 opaque, 1 fully transparent.
    pub transparency: f64,
    /// Presentation layer. `None` uses [`SOIL_LAYER`] or [`WATER_LAYER`].
    pub layer: Option<String>,
    pub psets: Vec<Pset>,
}

struct AssemblyGroup {
    name: String,
    psets: Vec<Pset>,
    products: Vec<Product>,
    xy: [f64; 2],
    top: f64,
    has_cylinder: bool,
}

/// One `IfcElementAssembly` per borehole. `Pset_Aufschluss` and
/// `Pset_Hyperlink` are written on the assembly and left on each part. The
/// assembly sits at the borehole XY and at the top of the uppermost cylinder.
pub fn solids_to_assemblies(solids: &[BoreholeSolid]) -> Result<Vec<BuildingExport>, String> {
    if solids.is_empty() {
        return Err("no borehole solids to write".into());
    }
    let mut order = Vec::new();
    let mut groups: HashMap<String, AssemblyGroup> = HashMap::new();
    for solid in solids {
        let key = group_key(solid);
        if !groups.contains_key(&key) {
            order.push(key.clone());
            groups.insert(
                key.clone(),
                AssemblyGroup {
                    name: assembly_name(solid),
                    psets: Vec::new(),
                    products: Vec::new(),
                    xy: [solid.position[0], solid.position[1]],
                    top: f64::NEG_INFINITY,
                    has_cylinder: false,
                },
            );
        }
        let group = groups.get_mut(&key).expect("group inserted above");
        let mut product = solid_to_product(solid)?;
        let shared = assembly_psets(&product.psets);
        product.parent = group.name.clone();
        merge_psets(&mut group.psets, shared);
        match solid.kind {
            SolidKind::Cylinder => {
                let z = solid.position[2] + solid.height;
                if !group.has_cylinder || z > group.top {
                    group.top = z;
                }
                group.has_cylinder = true;
            }
            SolidKind::Square if !group.has_cylinder && solid.position[2] > group.top => {
                group.top = solid.position[2];
            }
            SolidKind::Square => {}
        }
        group.products.push(product);
    }
    Ok(order
        .into_iter()
        .map(|key| {
            let group = groups.remove(&key).expect("group still present");
            let z = if group.top.is_finite() {
                group.top
            } else {
                0.0
            };
            BuildingExport {
                id: key,
                display_name: group.name,
                psets: group.psets,
                products: group.products,
                assembly: true,
                placement: Some([group.xy[0], group.xy[1], z]),
            }
        })
        .collect())
}

fn group_key(solid: &BoreholeSolid) -> String {
    let name = solid.borehole.trim();
    if name.is_empty() {
        format!("\u{0}{}", solid.name)
    } else {
        name.to_string()
    }
}

fn assembly_name(solid: &BoreholeSolid) -> String {
    let label = solid.borehole_name.trim();
    if !label.is_empty() {
        return label.to_string();
    }
    let key = solid.borehole.trim();
    if key.is_empty() {
        solid.name.clone()
    } else {
        key.to_string()
    }
}

fn assembly_psets(psets: &[Pset]) -> Vec<Pset> {
    psets
        .iter()
        .filter(|pset| {
            ASSEMBLY_PSET_NAMES
                .iter()
                .any(|name| name.eq_ignore_ascii_case(&pset.name))
        })
        .cloned()
        .collect()
}

fn merge_psets(dst: &mut Vec<Pset>, src: Vec<Pset>) {
    for pset in src {
        if !dst
            .iter()
            .any(|existing| existing.name.eq_ignore_ascii_case(&pset.name))
        {
            dst.push(pset);
        }
    }
}

fn solid_to_product(solid: &BoreholeSolid) -> Result<Product, String> {
    if solid.position.iter().any(|v| !v.is_finite()) {
        return Err(format!("{}: position is not finite", solid.name));
    }
    if !(0.0..=1.0).contains(&solid.transparency) {
        return Err(format!(
            "{}: transparency must be in 0..=1, got {}",
            solid.name, solid.transparency
        ));
    }
    if solid.color.iter().any(|v| !v.is_finite()) {
        return Err(format!("{}: color is not finite", solid.name));
    }
    let (vertices, faces) = match solid.kind {
        SolidKind::Cylinder => {
            if !(solid.height.is_finite() && solid.height > 0.0) {
                return Err(format!("{}: cylinder height must be > 0", solid.name));
            }
            if !(solid.radius.is_finite() && solid.radius > 0.0) {
                return Err(format!("{}: radius must be > 0", solid.name));
            }
            ngon_cylinder(solid.radius, solid.height, solid.segments.max(3))
        }
        SolidKind::Square => {
            if !(solid.size.is_finite() && solid.size > 0.0) {
                return Err(format!("{}: square size must be > 0", solid.name));
            }
            square(solid.size)
        }
    };
    let layer = solid.layer.clone().unwrap_or_else(|| match solid.kind {
        SolidKind::Cylinder => SOIL_LAYER.into(),
        SolidKind::Square => WATER_LAYER.into(),
    });
    Ok(Product {
        name: solid.name.clone(),
        ifc_type: "IfcBuildingElementProxy".into(),
        parent: "Boreholes".into(),
        vertices,
        faces,
        psets: solid.psets.clone(),
        color: Some(solid.color),
        layer: Some(layer),
        transparency: solid.transparency,
        extra: Vec::new(),
        placement: Some(solid.position),
    })
}

/// Horizontal square, centred on the origin, lying on z = 0. Same corners as
/// BoreholesGenericApp `_square_element_from_water`.
fn square(size: f64) -> (Vec<[f64; 3]>, Vec<Face>) {
    let h = 0.5 * size;
    (
        vec![[-h, -h, 0.0], [-h, h, 0.0], [h, h, 0.0], [h, -h, 0.0]],
        vec![Face::Simple(vec![0, 1, 2, 3])],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{self, ExportOptions};
    use crate::color::{Colors, BOREHOLE_UNDEFINED_RGB, GROUNDWATER_RGB};
    use crate::ifc::OwnerIdentity;
    use crate::types::PropValue;
    use std::collections::HashMap;

    fn related_list(line: &str) -> &str {
        let start = line.find("(#").expect("related objects");
        let end = line[start..].find(')').expect("list close") + start;
        &line[start..=end]
    }

    fn soil() -> BoreholeSolid {
        BoreholeSolid {
            borehole: "B.52".into(),
            borehole_name: String::new(),
            name: "B52_1".into(),
            kind: SolidKind::Cylinder,
            position: [565084.16, 5_934_034.654, -1.64],
            height: 1.2,
            size: 3.0,
            radius: 0.5,
            segments: 8,
            color: [1.0, 0.5, 0.0],
            transparency: 0.0,
            layer: Some("_Bodenaufschluesse_orange".into()),
            psets: vec![
                Pset::new(
                    "Pset_Aufschluss",
                    vec![("_Aufschlussnummer".into(), PropValue::label("B.52"))],
                ),
                Pset::new(
                    "Pset_Schicht",
                    vec![("_Bodenart".into(), PropValue::label("mS (Mittelsand)"))],
                ),
            ],
        }
    }

    fn water() -> BoreholeSolid {
        BoreholeSolid {
            borehole: "B.52".into(),
            borehole_name: String::new(),
            name: "B52_Grundwasser".into(),
            kind: SolidKind::Square,
            position: [565084.16, 5_934_034.654, 4.2],
            height: 0.0,
            size: 3.0,
            radius: 0.5,
            segments: 16,
            color: GROUNDWATER_RGB,
            transparency: GROUNDWATER_TRANSPARENCY,
            layer: None,
            psets: vec![
                Pset::new(
                    "Pset_Aufschluss",
                    vec![("_Aufschlussnummer".into(), PropValue::label("B.52"))],
                ),
                Pset::new(
                    "Pset_Hyperlink",
                    vec![("URL".into(), PropValue::label("https://example"))],
                ),
            ],
        }
    }

    #[test]
    fn cylinder_stands_on_its_lower_edge() {
        let product = solid_to_product(&soil()).unwrap();
        assert_eq!(product.placement, Some([565084.16, 5_934_034.654, -1.64]));
        assert!(product.vertices.iter().all(|v| v[2] >= 0.0 && v[2] <= 1.2));
        assert!(product.vertices.iter().any(|v| (v[2] - 1.2).abs() < 1e-9));
        assert!(product
            .vertices
            .iter()
            .all(|v| v[0].abs() <= 0.5 && v[1].abs() <= 0.5));
        assert_eq!(product.transparency, 0.0);
        assert_eq!(product.layer.as_deref(), Some("_Bodenaufschluesse_orange"));
    }

    #[test]
    fn square_is_three_metres_and_transparent() {
        let product = solid_to_product(&water()).unwrap();
        assert_eq!(product.vertices[0], [-1.5, -1.5, 0.0]);
        assert_eq!(product.vertices[2], [1.5, 1.5, 0.0]);
        assert_eq!(product.transparency, 0.8);
        assert_eq!(product.layer.as_deref(), Some(WATER_LAYER));
        assert_eq!(product.placement, Some([565084.16, 5_934_034.654, 4.2]));
    }

    #[test]
    fn zero_thickness_is_rejected() {
        let mut layer = soil();
        layer.height = 0.0;
        assert!(solid_to_product(&layer).unwrap_err().contains("height"));
    }

    #[test]
    fn transparency_outside_0_1_is_rejected() {
        let mut layer = soil();
        layer.transparency = 1.2;
        assert!(solid_to_product(&layer)
            .unwrap_err()
            .contains("transparency"));
    }

    #[test]
    fn written_file_has_colour_transparency_and_no_material() {
        let dir = std::env::temp_dir().join(format!("boreholes-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("boreholes.ifc");
        let opts = ExportOptions {
            mode: "mesh".into(),
            project_name: "Bohrungen".into(),
            site_name: "Site".into(),
            crs: None,
            styles: true,
            cad_layer: true,
            owner_history: true,
            owner: OwnerIdentity::default(),
            basepoint: Some((565084.16, 5_934_034.654, 0.0)),
            basepoint_size: 8.0,
            basepoint_psets: Vec::new(),
            psets: Vec::new(),
            ifc_types: HashMap::new(),
            colors: Colors::Defaults,
            progress: false,
        };
        api::boreholes_to_ifc(&[soil(), water()], path.to_str().unwrap(), &opts).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("IFCBUILDINGELEMENTPROXY("));
        assert!(!text.contains("IFCBUILDING("));
        assert!(!text.contains("IFCMATERIAL"));
        assert!(text.contains("IFCSURFACESTYLESHADING(#"));
        assert!(text.contains(",0.8);"));
        assert!(text.contains("'_Bodenaufschluesse_orange'"));
        assert!(text.contains("'_Grundwasser'"));
        assert!(text.contains("IFCLABEL('mS (Mittelsand)')"));
        assert!(text.contains("IFCCARTESIANPOINT((565084.16,5934034.654,-1.64));"));
        assert!(text.contains("IFCRELCONTAINEDINSPATIALSTRUCTURE("));
        assert_eq!(text.matches("IFCELEMENTASSEMBLY(").count(), 1);
        assert!(text.contains(".SITE.,.USERDEFINED.)"));
        assert!(text.contains("'B.52'"));
        assert_eq!(text.matches("'Pset_Aufschluss'").count(), 3);
        assert_eq!(text.matches("'Pset_Hyperlink'").count(), 2);
        assert!(text.contains("'Pset_Schicht'"));
        let contained: Vec<_> = text
            .lines()
            .filter(|line| line.contains("IFCRELCONTAINEDINSPATIALSTRUCTURE("))
            .collect();
        assert_eq!(contained.len(), 2);
        assert!(contained
            .iter()
            .all(|line| !related_list(line).contains(',')));
        assert!(text
            .lines()
            .any(|line| line.contains("IFCRELAGGREGATES(") && related_list(line).contains(',')));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn shared_borehole_is_one_assembly_at_the_cylinder_top() {
        let groups = solids_to_assemblies(&[soil(), water()]).unwrap();
        assert_eq!(groups.len(), 1);
        let group = &groups[0];
        assert!(group.assembly);
        assert_eq!(group.display_name, "B.52");
        assert_eq!(group.products.len(), 2);
        let [x, y, z] = group.placement.unwrap();
        assert_eq!([x, y], [565084.16, 5_934_034.654]);
        assert!((z - (-1.64 + 1.2)).abs() < 1e-9);
        assert!(group
            .psets
            .iter()
            .any(|pset| pset.name == "Pset_Aufschluss"));
        assert!(group.psets.iter().any(|pset| pset.name == "Pset_Hyperlink"));
        assert!(group.products.iter().all(|product| {
            product
                .psets
                .iter()
                .any(|pset| pset.name == "Pset_Aufschluss")
        }));
        assert!(group.products[1]
            .psets
            .iter()
            .any(|pset| pset.name == "Pset_Hyperlink"));
        assert!(group.products[0]
            .psets
            .iter()
            .any(|pset| pset.name == "Pset_Schicht"));
    }

    #[test]
    fn borehole_name_labels_the_assembly() {
        let mut layer = soil();
        layer.borehole = "BDHH_1".into();
        layer.borehole_name = "B 2".into();
        let mut marker = water();
        marker.borehole = "BDHH_1".into();
        marker.borehole_name = "ignored once grouped".into();
        let groups = solids_to_assemblies(&[layer, marker]).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].id, "BDHH_1");
        assert_eq!(groups[0].display_name, "B 2");
    }

    #[test]
    fn missing_borehole_key_keeps_solids_apart() {
        let mut layer = soil();
        layer.borehole.clear();
        let mut marker = water();
        marker.borehole.clear();
        let groups = solids_to_assemblies(&[layer, marker]).unwrap();
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn undefined_soil_colour_is_near_white() {
        assert!((BOREHOLE_UNDEFINED_RGB[0] - 254.0 / 255.0).abs() < 1e-12);
    }
}
