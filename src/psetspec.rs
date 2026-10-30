//! Declarative property-set specifications.
//!
//! A spec describes *rules*, not values. Python hands one over per run, it is
//! compiled into the enums below once, and then evaluated for every building
//! and every surface without any Python object crossing the boundary.

use std::collections::HashMap;

use crate::katalog::{dachform_label, funktion_label};
use crate::types::{upsert_pset, ParsedBuilding, PropValue, Pset, ValueType};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Building,
    Surface,
}

impl Scope {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "building" => Ok(Scope::Building),
            "surface" => Ok(Scope::Surface),
            other => Err(format!("unknown scope '{other}', expected 'building' or 'surface'")),
        }
    }
}

/// A field of the parsed building a property can read.
#[derive(Clone, Debug)]
pub enum AttrKey {
    Id,
    GmlName,
    Lod,
    Function,
    MeasuredHeight,
    StoreysAboveGround,
    RoofType,
    Extra(String),
}

impl AttrKey {
    pub fn parse(s: &str) -> Result<Self, String> {
        if let Some(name) = s.strip_prefix("extra:") {
            if name.is_empty() {
                return Err("attribute key 'extra:' needs a name after the colon".into());
            }
            return Ok(AttrKey::Extra(name.to_string()));
        }
        match s {
            "id" => Ok(AttrKey::Id),
            "gml_name" => Ok(AttrKey::GmlName),
            "lod" => Ok(AttrKey::Lod),
            "function" => Ok(AttrKey::Function),
            "measured_height" => Ok(AttrKey::MeasuredHeight),
            "storeys_above_ground" => Ok(AttrKey::StoreysAboveGround),
            "roof_type" => Ok(AttrKey::RoofType),
            other => Ok(AttrKey::Extra(other.to_string())),
        }
    }
}

/// A geometric quantity a surface-scope property can read.
#[derive(Clone, Copy, Debug)]
pub enum QtyKey {
    Area,
    Perimeter,
    Tilt,
    SurfaceType,
}

impl QtyKey {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "area" => Ok(QtyKey::Area),
            "perimeter" => Ok(QtyKey::Perimeter),
            "tilt" => Ok(QtyKey::Tilt),
            "surface_type" => Ok(QtyKey::SurfaceType),
            other => Err(format!(
                "unknown quantity key '{other}'; expected area, perimeter, tilt or surface_type"
            )),
        }
    }
}

/// Code → label mapping applied to a string attribute before it is written.
#[derive(Clone, Debug)]
pub enum Catalog {
    Funktion,
    Dachform,
    Custom(HashMap<String, String>),
}

impl Catalog {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "funktion" => Ok(Catalog::Funktion),
            "dachform" => Ok(Catalog::Dachform),
            other => Err(format!(
                "unknown catalog '{other}'; expected 'funktion', 'dachform' or a dict of codes"
            )),
        }
    }

    fn label(&self, code: &str) -> Option<String> {
        match self {
            Catalog::Funktion => Some(funktion_label(code).to_string()),
            Catalog::Dachform => Some(dachform_label(code).to_string()),
            Catalog::Custom(map) => map.get(code).cloned(),
        }
    }
}

/// A property rule. `ty` is the IFC type the caller declared; a source value
/// that does not fit it is dropped like a missing one, never retyped.
#[derive(Clone, Debug)]
pub enum Value {
    Const(PropValue),
    Attr {
        key: AttrKey,
        catalog: Option<Catalog>,
        decimals: Option<usize>,
        /// `None` drops the property when the attribute is absent.
        default: Option<String>,
        ty: ValueType,
    },
    Quantity {
        key: QtyKey,
        decimals: Option<usize>,
        ty: ValueType,
    },
}

#[derive(Clone, Debug)]
pub struct PsetSpec {
    pub name: String,
    pub scope: Scope,
    pub properties: Vec<(String, Value)>,
}

/// Geometry facts available to a surface-scope spec. In mesh mode these are the
/// building-wide aggregates.
pub struct SurfaceFacts<'a> {
    pub area: f64,
    pub perimeter: f64,
    pub tilt: f64,
    pub surface_type: &'a str,
}

/// Which quantities the surface-scope specs actually read, so the caller can
/// skip computing the ones nobody asked for.
#[derive(Clone, Copy, Debug, Default)]
pub struct Needs {
    pub area: bool,
    pub perimeter: bool,
    pub tilt: bool,
}

impl Needs {
    pub fn of(specs: &[PsetSpec]) -> Needs {
        let mut n = Needs::default();
        for spec in specs.iter().filter(|s| s.scope == Scope::Surface) {
            for (_, value) in &spec.properties {
                if let Value::Quantity { key, .. } = value {
                    match key {
                        QtyKey::Area => n.area = true,
                        QtyKey::Perimeter => n.perimeter = true,
                        QtyKey::Tilt => n.tilt = true,
                        QtyKey::SurfaceType => {}
                    }
                }
            }
        }
        n
    }
}

fn fmt_f64(v: f64, decimals: Option<usize>) -> String {
    match decimals {
        Some(d) => format!("{v:.prec$}", prec = d),
        None => format!("{v}"),
    }
}

impl Value {
    fn typed(&self, text: Option<String>) -> Option<PropValue> {
        let ty = match self {
            Value::Const(v) => return Some(v.clone()),
            Value::Attr { ty, .. } | Value::Quantity { ty, .. } => *ty,
        };
        PropValue::parse(ty, &text?).ok()
    }

    fn render(&self, b: &ParsedBuilding, facts: Option<&SurfaceFacts<'_>>) -> Option<PropValue> {
        self.typed(self.render_text(b, facts))
    }

    fn render_text(&self, b: &ParsedBuilding, facts: Option<&SurfaceFacts<'_>>) -> Option<String> {
        match self {
            Value::Const(_) => None,
            Value::Attr { key, catalog, decimals, default, .. } => {
                let raw = match key {
                    AttrKey::Id => Some(b.id.clone()),
                    AttrKey::GmlName => b.gml_name.clone(),
                    AttrKey::Lod => Some(b.lod.clone()),
                    AttrKey::Function => b.attributes.function.clone(),
                    AttrKey::RoofType => b.attributes.roof_type.clone(),
                    AttrKey::Extra(name) => b.attributes.extra.get(name).cloned(),
                    AttrKey::MeasuredHeight => {
                        b.attributes.measured_height.map(|h| fmt_f64(h, *decimals))
                    }
                    AttrKey::StoreysAboveGround => {
                        b.attributes.storeys_above_ground.map(|n| n.to_string())
                    }
                };
                // A catalog maps a code to a label; an unmapped code falls back
                // to the default rather than leaking the raw code.
                let mapped = match (raw, catalog) {
                    (Some(code), Some(cat)) => cat.label(&code),
                    (raw, _) => raw,
                };
                mapped.or_else(|| default.clone())
            }
            Value::Quantity { key, decimals, .. } => {
                let f = facts?;
                Some(match key {
                    QtyKey::Area => fmt_f64(f.area, *decimals),
                    QtyKey::Perimeter => fmt_f64(f.perimeter, *decimals),
                    QtyKey::Tilt => fmt_f64(f.tilt, *decimals),
                    QtyKey::SurfaceType => f.surface_type.to_string(),
                })
            }
        }
    }

    fn render_map(&self, attrs: &HashMap<String, String>) -> Option<PropValue> {
        self.typed(self.render_map_text(attrs))
    }

    fn render_map_text(&self, attrs: &HashMap<String, String>) -> Option<String> {
        match self {
            Value::Const(_) => None,
            Value::Attr { key, catalog, decimals, default, .. } => {
                let raw = match key {
                    AttrKey::Id => attrs.get("id").or_else(|| attrs.get("name")).cloned(),
                    AttrKey::GmlName => attrs.get("gml_name").cloned(),
                    AttrKey::Lod => attrs.get("lod").cloned(),
                    AttrKey::Function => attrs.get("function").cloned(),
                    AttrKey::MeasuredHeight => attrs.get("measured_height").cloned(),
                    AttrKey::StoreysAboveGround => attrs.get("storeys_above_ground").cloned(),
                    AttrKey::RoofType => attrs.get("roof_type").cloned(),
                    AttrKey::Extra(name) => attrs.get(name).cloned(),
                };
                let raw = raw.map(|s| match (decimals, s.parse::<f64>()) {
                    (Some(d), Ok(n)) => fmt_f64(n, Some(*d)),
                    _ => s,
                });
                let mapped = match (raw, catalog) {
                    (Some(code), Some(cat)) => cat.label(&code),
                    (raw, _) => raw,
                };
                mapped.or_else(|| default.clone())
            }
            Value::Quantity { .. } => None,
        }
    }
}

/// Evaluate every spec of `scope` for one building (and, for surface scope, one
/// surface). Properties whose value resolves to nothing are dropped, and the
/// declared order is preserved all the way into the IFC.
pub fn eval(
    specs: &[PsetSpec],
    scope: Scope,
    b: &ParsedBuilding,
    facts: Option<&SurfaceFacts<'_>>,
) -> Vec<Pset> {
    let mut out: Vec<Pset> = Vec::new();
    for spec in specs.iter().filter(|s| s.scope == scope) {
        let mut props = Vec::with_capacity(spec.properties.len());
        for (name, value) in &spec.properties {
            if let Some(rendered) = value.render(b, facts) {
                props.push((name.clone(), rendered));
            }
        }
        if !props.is_empty() {
            upsert_pset(&mut out, Pset::new(spec.name.clone(), props));
        }
    }
    out
}

/// Same rules as [`eval`], but values come from a flat attribute map (trees).
pub fn eval_map(specs: &[PsetSpec], scope: Scope, attrs: &HashMap<String, String>) -> Vec<Pset> {
    let mut out: Vec<Pset> = Vec::new();
    for spec in specs.iter().filter(|s| s.scope == scope) {
        let mut props = Vec::with_capacity(spec.properties.len());
        for (name, value) in &spec.properties {
            if let Some(rendered) = value.render_map(attrs) {
                props.push((name.clone(), rendered));
            }
        }
        if !props.is_empty() {
            upsert_pset(&mut out, Pset::new(spec.name.clone(), props));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{BuildingAttributes, ParsedBuilding};

    fn building() -> ParsedBuilding {
        ParsedBuilding {
            id: "BLDG_1".into(),
            gml_name: None,
            lod: "LoD2".into(),
            attributes: BuildingAttributes {
                function: Some("31001_1000".into()),
                measured_height: Some(31.675),
                storeys_above_ground: Some(8),
                roof_type: Some("3100".into()),
                extra: HashMap::from([("Gemeinde".to_string(), "Hamburg".to_string())]),
            },
            boundaries: Vec::new(),
        }
    }

    fn spec(props: Vec<(String, Value)>) -> Vec<PsetSpec> {
        vec![PsetSpec { name: "P".into(), scope: Scope::Building, properties: props }]
    }

    fn props_of(psets: &[Pset], name: &str) -> HashMap<String, String> {
        psets
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.properties.iter().map(|(k, v)| (k.clone(), v.to_string())).collect())
            .unwrap_or_default()
    }

    fn order_of(psets: &[Pset], name: &str) -> Vec<String> {
        psets
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.properties.iter().map(|(k, _)| k.clone()).collect())
            .unwrap_or_default()
    }

    #[test]
    fn renders_const_attr_and_catalog() {
        let specs = spec(vec![
            ("c".into(), Value::Const(PropValue::label("k"))),
            (
                "lod".into(),
                Value::Attr { key: AttrKey::Lod, catalog: None, decimals: None, default: None, ty: ValueType::Label },
            ),
            (
                "h".into(),
                Value::Attr {
                    key: AttrKey::MeasuredHeight,
                    catalog: None,
                    decimals: Some(2),
                    default: None,
                    ty: ValueType::Label,
                },
            ),
            (
                "fn".into(),
                Value::Attr {
                    key: AttrKey::Function,
                    catalog: Some(Catalog::Funktion),
                    decimals: None,
                    default: Some("undefiniert".into()),
                    ty: ValueType::Label,
                },
            ),
            (
                "extra".into(),
                Value::Attr {
                    key: AttrKey::Extra("Gemeinde".into()),
                    catalog: None,
                    decimals: None,
                    default: None,
                    ty: ValueType::Label,
                },
            ),
        ]);
        let out = eval(&specs, Scope::Building, &building(), None);
        let p = props_of(&out, "P");
        assert_eq!(p["c"], "k");
        assert_eq!(p["lod"], "LoD2");
        assert_eq!(p["h"], "31.68");
        assert_eq!(p["extra"], "Hamburg");
        assert_ne!(p["fn"], "31001_1000");
    }

    #[test]
    fn missing_attribute_uses_default_or_is_dropped() {
        let mut b = building();
        b.attributes.measured_height = None;
        let specs = spec(vec![
            (
                "with_default".into(),
                Value::Attr {
                    key: AttrKey::MeasuredHeight,
                    catalog: None,
                    decimals: None,
                    default: Some("0.0".into()),
                    ty: ValueType::Label,
                },
            ),
            (
                "dropped".into(),
                Value::Attr {
                    key: AttrKey::MeasuredHeight,
                    catalog: None,
                    decimals: None,
                    default: None,
                    ty: ValueType::Label,
                },
            ),
        ]);
        let out = eval(&specs, Scope::Building, &b, None);
        let p = props_of(&out, "P");
        assert_eq!(p["with_default"], "0.0");
        assert!(!p.contains_key("dropped"));
        // The dropped property must not leave a hole in the order either.
        assert_eq!(order_of(&out, "P"), vec!["with_default"]);
    }

    #[test]
    fn needs_only_reports_requested_quantities() {
        let specs = vec![PsetSpec {
            name: "Q".into(),
            scope: Scope::Surface,
            properties: vec![
                ("A".into(), Value::Quantity { key: QtyKey::Area, decimals: Some(4), ty: ValueType::Label }),
                ("T".into(), Value::Quantity { key: QtyKey::SurfaceType, decimals: None, ty: ValueType::Label }),
            ],
        }];
        let n = Needs::of(&specs);
        assert!(n.area && !n.perimeter && !n.tilt);
    }

    #[test]
    fn building_scope_ignores_surface_specs() {
        let specs = vec![PsetSpec {
            name: "Q".into(),
            scope: Scope::Surface,
            properties: vec![("A".into(), Value::Quantity { key: QtyKey::Area, decimals: None, ty: ValueType::Label })],
        }];
        assert!(eval(&specs, Scope::Building, &building(), None).is_empty());
    }

    #[test]
    fn declared_property_order_is_preserved() {
        let names = ["z_last", "a_middle", "m_first"];
        let specs = spec(
            names
                .iter()
                .map(|n| ((*n).to_string(), Value::Const(PropValue::label("v"))))
                .collect(),
        );
        let out = eval(&specs, Scope::Building, &building(), None);
        assert_eq!(order_of(&out, "P"), names);
    }

    #[test]
    fn later_spec_of_same_name_replaces_earlier() {
        let specs = vec![
            PsetSpec {
                name: "P".into(),
                scope: Scope::Building,
                properties: vec![("a".into(), Value::Const(PropValue::label("1")))],
            },
            PsetSpec {
                name: "P".into(),
                scope: Scope::Building,
                properties: vec![("b".into(), Value::Const(PropValue::label("2")))],
            },
        ];
        let out = eval(&specs, Scope::Building, &building(), None);
        assert_eq!(out.len(), 1);
        assert_eq!(order_of(&out, "P"), vec!["b"]);
    }

    #[test]
    fn rejects_unknown_keys() {
        assert!(matches!(AttrKey::parse("nope"), Ok(AttrKey::Extra(n)) if n == "nope"));
        assert!(AttrKey::parse("extra:").is_err());
        assert!(QtyKey::parse("volume").is_err());
        assert!(Scope::parse("product").is_err());
        assert!(AttrKey::parse("extra:Gemeinde").is_ok());
    }

    #[test]
    fn eval_map_reads_tree_attributes() {
        let specs = spec(vec![
            ("_Gattung".into(), Value::Const(PropValue::label("Gehoelz"))),
            (
                "_Baumnummer".into(),
                Value::Attr {
                    key: AttrKey::Extra("baumnummer".into()),
                    catalog: None,
                    decimals: None,
                    default: None,
                    ty: ValueType::Label,
                },
            ),
            (
                "_Kronendurchmesser".into(),
                Value::Attr {
                    key: AttrKey::Extra("kronendurchmesser".into()),
                    catalog: None,
                    decimals: Some(2),
                    default: None,
                    ty: ValueType::Label,
                },
            ),
            (
                "_Missing".into(),
                Value::Attr {
                    key: AttrKey::Extra("nope".into()),
                    catalog: None,
                    decimals: None,
                    default: None,
                    ty: ValueType::Label,
                },
            ),
        ]);
        let attrs = HashMap::from([
            ("baumnummer".into(), "001".into()),
            ("kronendurchmesser".into(), "4.0".into()),
        ]);
        let p = props_of(&eval_map(&specs, Scope::Building, &attrs), "P");
        assert_eq!(p["_Gattung"], "Gehoelz");
        assert_eq!(p["_Baumnummer"], "001");
        assert_eq!(p["_Kronendurchmesser"], "4.00");
        assert!(!p.contains_key("_Missing"));
    }

    #[test]
    fn declared_type_is_kept_and_misfits_are_dropped() {
        let attr = |key: AttrKey, ty: ValueType| Value::Attr {
            key,
            catalog: None,
            decimals: None,
            default: None,
            ty,
        };
        let specs = spec(vec![
            ("real".into(), attr(AttrKey::MeasuredHeight, ValueType::Real)),
            ("int".into(), attr(AttrKey::StoreysAboveGround, ValueType::Integer)),
            ("misfit".into(), attr(AttrKey::Lod, ValueType::Integer)),
            ("label".into(), attr(AttrKey::StoreysAboveGround, ValueType::Label)),
        ]);
        let out = eval(&specs, Scope::Building, &building(), None);
        let props: HashMap<_, _> = out[0].properties.iter().cloned().collect();
        assert_eq!(props["real"], PropValue::Real(31.675));
        assert_eq!(props["int"], PropValue::Integer(8));
        assert_eq!(props["label"], PropValue::label("8"));
        assert!(!props.contains_key("misfit"));
    }
}
