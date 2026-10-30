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

/// IFC type of a property value. The caller declares it; Rust never guesses it
/// from the text, so a property has the same type on every object.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ValueType {
    #[default]
    Label,
    Text,
    Integer,
    Real,
    Boolean,
    LengthMeasure,
    AreaMeasure,
}

impl ValueType {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "IfcLabel" => Ok(ValueType::Label),
            "IfcText" => Ok(ValueType::Text),
            "IfcInteger" => Ok(ValueType::Integer),
            "IfcReal" => Ok(ValueType::Real),
            "IfcBoolean" => Ok(ValueType::Boolean),
            "IfcLengthMeasure" => Ok(ValueType::LengthMeasure),
            "IfcAreaMeasure" => Ok(ValueType::AreaMeasure),
            other => Err(format!(
                "unknown value type '{other}'; expected IfcLabel, IfcText, IfcInteger, IfcReal, \
                 IfcBoolean, IfcLengthMeasure or IfcAreaMeasure"
            )),
        }
    }
}

/// A property value with its IFC type. Measures are in metre / square metre,
/// the units the writer declares.
#[derive(Clone, Debug, PartialEq)]
pub enum PropValue {
    Label(String),
    Text(String),
    Integer(i64),
    Real(f64),
    Boolean(bool),
    LengthMeasure(f64),
    AreaMeasure(f64),
}

impl PropValue {
    pub fn label(value: impl Into<String>) -> Self {
        PropValue::Label(value.into())
    }

    /// `text` as a value of type `ty`, or an error if it does not fit.
    pub fn parse(ty: ValueType, text: &str) -> Result<Self, String> {
        let number = || {
            text.trim()
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or_else(|| format!("'{text}' is not a number"))
        };
        Ok(match ty {
            ValueType::Label => PropValue::Label(text.to_string()),
            ValueType::Text => PropValue::Text(text.to_string()),
            ValueType::Integer => PropValue::Integer(
                text.trim().parse().map_err(|_| format!("'{text}' is not an integer"))?,
            ),
            ValueType::Real => PropValue::Real(number()?),
            ValueType::LengthMeasure => PropValue::LengthMeasure(number()?),
            ValueType::AreaMeasure => PropValue::AreaMeasure(number()?),
            ValueType::Boolean => match text.trim().to_ascii_lowercase().as_str() {
                "true" => PropValue::Boolean(true),
                "false" => PropValue::Boolean(false),
                _ => return Err(format!("'{text}' is not a boolean")),
            },
        })
    }
}

impl std::fmt::Display for PropValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PropValue::Label(s) | PropValue::Text(s) => f.write_str(s),
            PropValue::Integer(i) => write!(f, "{i}"),
            PropValue::Boolean(b) => write!(f, "{b}"),
            PropValue::Real(v) | PropValue::LengthMeasure(v) | PropValue::AreaMeasure(v) => write!(f, "{v}"),
        }
    }
}

/// One property set: its name plus properties in a stable, caller-defined
/// order, which is what makes the written IFC byte-reproducible.
#[derive(Clone, Debug)]
pub struct Pset {
    pub name: String,
    pub properties: Vec<(String, PropValue)>,
}

impl Pset {
    pub fn new(name: impl Into<String>, properties: Vec<(String, PropValue)>) -> Self {
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

/// Extra body on the same product (tree crown next to the trunk).
#[derive(Clone, Debug)]
pub struct ProductPart {
    pub vertices: Vec<[f64; 3]>,
    pub faces: Vec<Face>,
    pub color: Option<[f64; 3]>,
    pub layer: Option<String>,
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
    pub layer: Option<String>,
    pub extra: Vec<ProductPart>,
    /// When set, geometry stays local and this is written as `IfcLocalPlacement`.
    pub placement: Option<[f64; 3]>,
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
