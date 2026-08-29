use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct Boundary {
    pub ring: Vec<[f64; 3]>,
    pub interiors: Vec<Vec<[f64; 3]>>,
    pub surface_type: String,
    pub source_part_id: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct BuildingAttributes {
    pub function: Option<String>,
    pub measured_height: Option<f64>,
    pub storeys_above_ground: Option<i64>,
    pub roof_type: Option<String>,
    pub extra: HashMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct ParsedBuilding {
    pub id: String,
    pub gml_name: Option<String>,
    pub lod: String,
    pub attributes: BuildingAttributes,
    pub boundaries: Vec<Boundary>,
}

#[derive(Clone, Debug)]
pub enum Face {
    Simple(Vec<u32>),
    WithVoids { outer: Vec<u32>, inners: Vec<Vec<u32>> },
}

/// One property set: its name plus properties in a stable, caller-defined
/// order, which is what makes the written IFC byte-reproducible.
#[derive(Clone, Debug)]
pub struct Pset {
    pub name: String,
    pub properties: Vec<(String, String)>,
}

impl Pset {
    pub fn new(name: impl Into<String>, properties: Vec<(String, String)>) -> Self {
        Self { name: name.into(), properties }
    }

    pub fn is_empty(&self) -> bool {
        self.properties.is_empty()
    }
}

/// Replace a set of the same name, otherwise append. Keeps insertion order and
/// lets the last writer win, as the previous map-based psets did.
pub fn upsert_pset(psets: &mut Vec<Pset>, pset: Pset) {
    match psets.iter_mut().find(|p| p.name == pset.name) {
        Some(existing) => *existing = pset,
        None => psets.push(pset),
    }
}

#[derive(Clone, Debug)]
pub struct Product {
    pub name: String,
    pub ifc_type: String,
    pub parent: String,
    pub vertices: Vec<[f64; 3]>,
    pub faces: Vec<Face>,
    pub psets: Vec<Pset>,
    pub color: Option<[f64; 3]>,
}

#[derive(Clone, Debug)]
pub struct BuildingExport {
    pub id: String,
    pub display_name: String,
    pub psets: Vec<Pset>,
    pub products: Vec<Product>,
}

pub fn strip_close(pts: &mut Vec<[f64; 3]>) {
    if pts.len() > 1 && pts.first() == pts.last() {
        pts.pop();
    }
}

impl Boundary {
    pub fn cleaned(mut self) -> Option<Self> {
        strip_close(&mut self.ring);
        self.interiors.retain_mut(|ring| {
            strip_close(ring);
            ring.len() >= 3
        });
        if self.ring.len() >= 3 {
            Some(self)
        } else {
            None
        }
    }
}

/// Used when the caller omits `ifc_types` or the surface is not in the map.
pub const FALLBACK_IFC_TYPE: &str = "IfcBuildingElementProxy";

/// Look up a CityGML/CityJSON surface (or `"Building"` in mesh mode).
/// Unmapped keys become [`FALLBACK_IFC_TYPE`] — there is no built-in CityGML map.
pub fn lookup_ifc_type<'a>(map: &'a HashMap<String, String>, key: &str) -> &'a str {
    map.get(key).map(String::as_str).unwrap_or(FALLBACK_IFC_TYPE)
}

pub fn building_display_name(b: &ParsedBuilding) -> String {
    match b.gml_name.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(name) => format!("{name}_{}", b.id),
        None => b.id.clone(),
    }
}
