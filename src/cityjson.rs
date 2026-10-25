//! CityJSON 1.1 → [`ParsedBuilding`] (same export path as CityGML).
//!
//! No HTTP. Callers (examples, later BIMFabrik) fetch or load a file and
//! pass the document text or path.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::parse::BboxUtm;
use crate::types::{Boundary, BuildingAttributes, ParsedBuilding};

pub fn parse_cityjson_file(
    path: impl AsRef<Path>,
    building_id_filter: Option<&str>,
    bbox_epsg: Option<BboxUtm>,
) -> Result<Vec<ParsedBuilding>, String> {
    let path = path.as_ref();
    let text = fs::read_to_string(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    parse_cityjson_str(&text, building_id_filter, bbox_epsg)
}

pub fn parse_cityjson_str(
    text: &str,
    building_id_filter: Option<&str>,
    bbox_epsg: Option<BboxUtm>,
) -> Result<Vec<ParsedBuilding>, String> {
    parse_cityjson_value(&parse_cityjson_json(text)?, building_id_filter, bbox_epsg)
}

/// The JSON tree of a CityJSON document, for [`parse_cityjson_value`].
pub fn parse_cityjson_json(text: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|e| format!("CityJSON: {e}"))
}

pub fn parse_cityjson_value(
    root: &Value,
    building_id_filter: Option<&str>,
    bbox_epsg: Option<BboxUtm>,
) -> Result<Vec<ParsedBuilding>, String> {
    if root.get("type").and_then(Value::as_str) != Some("CityJSON")
        && root.get("type").and_then(Value::as_str) != Some("CityJSONFeature")
    {
        return Err("not a CityJSON document".into());
    }
    let verts = decode_vertices(root)?;
    let objects = root
        .get("CityObjects")
        .and_then(Value::as_object)
        .ok_or_else(|| "CityJSON has no CityObjects".to_string())?;

    let mut by_id: HashMap<String, ParsedBuilding> = HashMap::new();
    let mut parts: Vec<(String, ParsedBuilding)> = Vec::new();

    for (id, obj) in objects {
        let typ = obj.get("type").and_then(Value::as_str).unwrap_or("");
        if typ != "Building" && typ != "BuildingPart" {
            continue;
        }
        if let Some(want) = building_id_filter {
            if typ == "Building" && id != want {
                continue;
            }
        }
        let Some(building) = object_to_building(id, obj, &verts) else {
            continue;
        };
        if typ == "BuildingPart" {
            let parent = obj
                .get("parents")
                .and_then(Value::as_array)
                .and_then(|a| a.first())
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            parts.push((parent, building));
        } else {
            by_id.insert(id.clone(), building);
        }
    }

    for (parent, mut part) in parts {
        if let Some(want) = building_id_filter {
            if parent != want && part.id != want {
                continue;
            }
        }
        if let Some(host) = by_id.get_mut(&parent) {
            for b in &mut part.boundaries {
                if b.source_part_id.is_none() {
                    b.source_part_id = Some(part.id.clone());
                }
            }
            host.boundaries.extend(part.boundaries);
            merge_attrs(&mut host.attributes, &part.attributes);
        } else if building_id_filter.is_none() || building_id_filter == Some(part.id.as_str()) {
            by_id.insert(part.id.clone(), part);
        }
    }

    let mut out: Vec<ParsedBuilding> = by_id.into_values().collect();
    out.retain(|b| !b.boundaries.is_empty());
    if let Some(bb) = bbox_epsg {
        out.retain(|b| building_hits_bbox(b, bb));
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

fn decode_vertices(root: &Value) -> Result<Vec<[f64; 3]>, String> {
    let raw = root
        .get("vertices")
        .and_then(Value::as_array)
        .ok_or_else(|| "CityJSON has no vertices".to_string())?;
    let (sx, sy, sz, tx, ty, tz) = match root.get("transform") {
        Some(t) => {
            let scale = t.get("scale").and_then(Value::as_array);
            let tr = t.get("translate").and_then(Value::as_array);
            (
                num(scale, 0, 1.0),
                num(scale, 1, 1.0),
                num(scale, 2, 1.0),
                num(tr, 0, 0.0),
                num(tr, 1, 0.0),
                num(tr, 2, 0.0),
            )
        }
        None => (1.0, 1.0, 1.0, 0.0, 0.0, 0.0),
    };
    let mut out = Vec::with_capacity(raw.len());
    for v in raw {
        let a = v.as_array().ok_or_else(|| "vertex is not an array".to_string())?;
        if a.len() < 3 {
            return Err("vertex needs 3 coordinates".into());
        }
        out.push([
            as_f64(&a[0]) * sx + tx,
            as_f64(&a[1]) * sy + ty,
            as_f64(&a[2]) * sz + tz,
        ]);
    }
    Ok(out)
}

fn object_to_building(id: &str, obj: &Value, verts: &[[f64; 3]]) -> Option<ParsedBuilding> {
    let mut boundaries = Vec::new();
    let mut lod = "LoD2".to_string();
    if let Some(geoms) = obj.get("geometry").and_then(Value::as_array) {
        for g in geoms {
            if let Some(l) = geom_lod(g) {
                lod = l;
            }
            boundaries.extend(geom_to_boundaries(g, verts, None));
        }
    }
    if boundaries.is_empty() {
        return None;
    }
    Some(ParsedBuilding {
        id: id.to_string(),
        gml_name: address_name(obj),
        lod,
        attributes: attrs_from_obj(obj),
        boundaries,
    })
}

fn lod_label(n: f64) -> String {
    if n >= 2.5 {
        "LoD3".into()
    } else if n < 1.5 {
        "LoD1".into()
    } else {
        "LoD2".into()
    }
}

fn geom_lod(g: &Value) -> Option<String> {
    let lod = g.get("lod")?;
    if let Some(s) = lod.as_str() {
        let digits = s.trim().trim_start_matches(['L', 'o', 'D', 'l', 'd']);
        if let Ok(n) = digits.parse::<f64>() {
            return Some(lod_label(n));
        }
    }
    lod.as_f64().map(lod_label)
}

fn geom_to_boundaries(g: &Value, verts: &[[f64; 3]], part_id: Option<String>) -> Vec<Boundary> {
    let typ = g.get("type").and_then(Value::as_str).unwrap_or("");
    let Some(bounds) = g.get("boundaries") else {
        return Vec::new();
    };
    let faces = collect_faces(typ, bounds);
    let labels = semantic_labels(g, faces.len());
    let mut out = Vec::new();
    for (face, label) in faces.into_iter().zip(labels) {
        let (outer_idx, inner_idx) = face;
        let ring = ring_xyz(&outer_idx, verts);
        let interiors = inner_idx
            .into_iter()
            .map(|r| ring_xyz(&r, verts))
            .collect();
        if let Some(b) = (Boundary {
            ring,
            interiors,
            surface_type: label,
            source_part_id: part_id.clone(),
        })
        .cleaned()
        {
            out.push(b);
        }
    }
    out
}

/// Returns (outer index ring, inner index rings) per semantic face.
fn collect_faces(typ: &str, boundaries: &Value) -> Vec<(Vec<u32>, Vec<Vec<u32>>)> {
    match typ {
        "MultiSurface" | "CompositeSurface" => faces_from_surface(boundaries),
        "Solid" => faces_from_solid(boundaries),
        "MultiSolid" | "CompositeSolid" => {
            let mut all = Vec::new();
            if let Some(solids) = boundaries.as_array() {
                for solid in solids {
                    all.extend(faces_from_solid(solid));
                }
            }
            all
        }
        _ => Vec::new(),
    }
}

fn faces_from_solid(boundaries: &Value) -> Vec<(Vec<u32>, Vec<Vec<u32>>)> {
    let mut out = Vec::new();
    let Some(shells) = boundaries.as_array() else {
        return out;
    };
    for shell in shells {
        out.extend(faces_from_surface(shell));
    }
    out
}

fn faces_from_surface(surfaces: &Value) -> Vec<(Vec<u32>, Vec<Vec<u32>>)> {
    let mut out = Vec::new();
    let Some(faces) = surfaces.as_array() else {
        return out;
    };
    for face in faces {
        let Some(rings) = face.as_array() else {
            continue;
        };
        if rings.is_empty() {
            continue;
        }
        let outer = index_ring(&rings[0]);
        if outer.len() < 3 {
            continue;
        }
        let inners = rings[1..].iter().map(index_ring).filter(|r| r.len() >= 3).collect();
        out.push((outer, inners));
    }
    out
}

fn index_ring(v: &Value) -> Vec<u32> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|n| n.as_u64().or_else(|| n.as_i64().map(|i| i as u64)))
                .map(|i| i as u32)
                .collect()
        })
        .unwrap_or_default()
}

fn ring_xyz(idx: &[u32], verts: &[[f64; 3]]) -> Vec<[f64; 3]> {
    idx.iter()
        .filter_map(|&i| verts.get(i as usize).copied())
        .collect()
}

fn semantic_labels(g: &Value, nfaces: usize) -> Vec<String> {
    let mut labels = vec!["Unknown".to_string(); nfaces];
    let Some(sem) = g.get("semantics") else {
        return labels;
    };
    let surfaces = sem.get("surfaces").and_then(Value::as_array);
    let values = sem.get("values");
    let ids = flatten_semantic_values(values);
    for (i, sid) in ids.into_iter().enumerate().take(nfaces) {
        if let (Some(surfaces), Some(si)) = (surfaces, sid) {
            if let Some(name) = surfaces
                .get(si)
                .and_then(|s| s.get("type"))
                .and_then(Value::as_str)
            {
                labels[i] = name.to_string();
            }
        }
    }
    labels
}

fn flatten_semantic_values(v: Option<&Value>) -> Vec<Option<usize>> {
    let mut out = Vec::new();
    fn walk(v: &Value, out: &mut Vec<Option<usize>>) {
        match v {
            Value::Null => out.push(None),
            Value::Number(n) => out.push(n.as_u64().map(|i| i as usize)),
            Value::Array(a) => {
                for x in a {
                    walk(x, out);
                }
            }
            _ => {}
        }
    }
    if let Some(v) = v {
        walk(v, &mut out);
    }
    out
}

fn attrs_from_obj(obj: &Value) -> BuildingAttributes {
    let a = obj.get("attributes");
    let mut out = BuildingAttributes::default();
    if let Some(a) = a {
        if let Some(s) = a.get("function").and_then(as_string) {
            out.function = Some(s);
        }
        if let Some(h) = a.get("measuredHeight").and_then(Value::as_f64) {
            out.measured_height = Some(h);
        }
        if let Some(n) = a
            .get("storeysAboveGround")
            .and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)))
        {
            out.storeys_above_ground = Some(n);
        }
        if let Some(s) = a.get("roofType").and_then(as_string) {
            out.roof_type = Some(s);
        }
        if let Some(map) = a.as_object() {
            for (k, v) in map {
                if matches!(
                    k.as_str(),
                    "function" | "measuredHeight" | "storeysAboveGround" | "roofType"
                ) {
                    continue;
                }
                if let Some(s) = as_string(v) {
                    out.extra.insert(k.clone(), s);
                }
            }
        }
    }
    out
}

fn address_name(obj: &Value) -> Option<String> {
    let addr = obj.get("address").and_then(Value::as_array)?.first()?;
    let street = addr.get("ThoroughfareName").and_then(Value::as_str).unwrap_or("").trim();
    let num = addr.get("ThoroughfareNumber").and_then(Value::as_str).unwrap_or("").trim();
    let name = format!("{street} {num}").trim().to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn merge_attrs(dst: &mut BuildingAttributes, src: &BuildingAttributes) {
    if dst.function.is_none() {
        dst.function = src.function.clone();
    }
    if dst.measured_height.is_none() {
        dst.measured_height = src.measured_height;
    }
    if dst.storeys_above_ground.is_none() {
        dst.storeys_above_ground = src.storeys_above_ground;
    }
    if dst.roof_type.is_none() {
        dst.roof_type = src.roof_type.clone();
    }
}

fn building_hits_bbox(b: &ParsedBuilding, bb: BboxUtm) -> bool {
    let (xmin, ymin, xmax, ymax) = bb;
    b.boundaries.iter().any(|bd| {
        bd.ring.iter().any(|p| p[0] >= xmin && p[0] <= xmax && p[1] >= ymin && p[1] <= ymax)
    })
}

fn num(arr: Option<&Vec<Value>>, i: usize, default: f64) -> f64 {
    arr.and_then(|a| a.get(i)).map(as_f64).unwrap_or(default)
}

fn as_f64(v: &Value) -> f64 {
    v.as_f64()
        .or_else(|| v.as_i64().map(|i| i as f64))
        .or_else(|| v.as_u64().map(|i| i as f64))
        .unwrap_or(0.0)
}

fn as_string(v: &Value) -> Option<String> {
    if let Some(s) = v.as_str() {
        return Some(s.to_string());
    }
    if let Some(n) = v.as_i64() {
        return Some(n.to_string());
    }
    if let Some(n) = v.as_f64() {
        return Some(format!("{n}"));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINI: &str = r#"{
      "type":"CityJSON","version":"1.1",
      "transform":{"scale":[1,1,1],"translate":[0,0,0]},
      "vertices":[[0,0,0],[10,0,0],[10,8,0],[0,8,0],[0,0,4],[10,0,4],[10,8,4],[0,8,4]],
      "CityObjects":{"B1":{
        "type":"Building",
        "attributes":{"function":"31001_1010","measuredHeight":4.0,"roofType":"3100"},
        "geometry":[{
          "type":"Solid","lod":"2",
          "boundaries":[[
            [[0,1,2,3,0]],
            [[4,7,6,5,4]],
            [[0,4,5,1,0]]
          ]],
          "semantics":{
            "surfaces":[{"type":"GroundSurface"},{"type":"RoofSurface"},{"type":"WallSurface"}],
            "values":[[0,1,2]]
          }
        }]
      }}
    }"#;

    #[test]
    fn parses_solid_semantics() {
        let b = parse_cityjson_str(MINI, None, None).unwrap();
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].id, "B1");
        assert_eq!(b[0].boundaries.len(), 3);
        assert_eq!(b[0].boundaries[0].surface_type, "GroundSurface");
        assert_eq!(b[0].boundaries[1].surface_type, "RoofSurface");
        assert_eq!(b[0].attributes.function.as_deref(), Some("31001_1010"));
        assert!((b[0].boundaries[0].ring[1][0] - 10.0).abs() < 1e-9);
    }
}
