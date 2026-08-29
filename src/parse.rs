//! Streaming CityGML 1.0 / 2.0 building parser (LoD1–3).

use std::collections::HashMap;
use std::path::Path;

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::types::{Boundary, BuildingAttributes, ParsedBuilding};

pub type BboxUtm = (f64, f64, f64, f64);

fn local(raw: &[u8]) -> &[u8] {
    if let Some(i) = raw.iter().rposition(|&b| b == b'}' || b == b':') {
        &raw[i + 1..]
    } else {
        raw
    }
}

fn local_str(raw: &[u8]) -> String {
    String::from_utf8_lossy(local(raw)).into_owned()
}

fn parse_poslist(text: &str) -> Vec<[f64; 3]> {
    let nums: Vec<f64> = text
        .split_whitespace()
        .filter_map(|s| s.parse().ok())
        .collect();
    nums.as_chunks::<3>().0.to_vec()
}

fn attr_local_value(attrs: quick_xml::events::attributes::Attributes<'_>, want: &[u8]) -> Option<String> {
    for attr in attrs.flatten() {
        if local(attr.key.as_ref()) == want {
            return Some(String::from_utf8_lossy(&attr.value).into_owned());
        }
    }
    None
}

const SURFACE_TYPES: &[&[u8]] = &[
    b"RoofSurface",
    b"WallSurface",
    b"GroundSurface",
    b"OuterFloorSurface",
    b"OuterCeilingSurface",
    b"ClosureSurface",
];

struct BuildingAcc {
    id: String,
    gml_name: Option<String>,
    saw_lod1: bool,
    saw_lod3: bool,
    /// `lod3Solid` / terrain curves — xlink or linework, not extra rings.
    skip_geom: bool,
    attributes: BuildingAttributes,
    boundaries: Vec<Boundary>,
    skip: bool,
    part_id: Option<String>,
    surface_type: Option<String>,
    in_bounded_by: bool,
    in_lod1: bool,
    in_exterior: bool,
    in_interior: bool,
    exterior: Vec<[f64; 3]>,
    interiors: Vec<Vec<[f64; 3]>>,
    buf: String,
    text_tag: Option<String>,
    current_poly_id: Option<String>,
    generic_name: Option<String>,
    polygons_by_id: HashMap<String, Boundary>,
    xlink_refs: Vec<String>,
    xs: (f64, f64),
    ys: (f64, f64),
}

impl BuildingAcc {
    fn new(id: String, skip: bool) -> Self {
        Self {
            id,
            gml_name: None,
            saw_lod1: false,
            saw_lod3: false,
            skip_geom: false,
            attributes: BuildingAttributes::default(),
            boundaries: Vec::new(),
            skip,
            part_id: None,
            surface_type: None,
            in_bounded_by: false,
            in_lod1: false,
            in_exterior: false,
            in_interior: false,
            exterior: Vec::new(),
            interiors: Vec::new(),
            buf: String::new(),
            text_tag: None,
            current_poly_id: None,
            generic_name: None,
            polygons_by_id: HashMap::new(),
            xlink_refs: Vec::new(),
            xs: (f64::MAX, f64::MIN),
            ys: (f64::MAX, f64::MIN),
        }
    }

    fn note_points(&mut self, pts: &[[f64; 3]]) {
        for p in pts {
            self.xs.0 = self.xs.0.min(p[0]);
            self.xs.1 = self.xs.1.max(p[0]);
            self.ys.0 = self.ys.0.min(p[1]);
            self.ys.1 = self.ys.1.max(p[1]);
        }
    }

    fn outside_bbox(&self, bbox: BboxUtm) -> bool {
        self.xs.1 < bbox.0 || self.ys.1 < bbox.1 || self.xs.0 > bbox.2 || self.ys.0 > bbox.3
    }

    fn finish_polygon(&mut self) {
        if self.skip || self.skip_geom {
            self.exterior.clear();
            self.interiors.clear();
            self.current_poly_id = None;
            return;
        }
        let surface = self.surface_type.clone().unwrap_or_else(|| {
            if self.saw_lod1 {
                "ClosureSurface".into()
            } else {
                "UnknownSurface".into()
            }
        });
        let boundary = Boundary {
            ring: std::mem::take(&mut self.exterior),
            interiors: std::mem::take(&mut self.interiors),
            surface_type: surface,
            source_part_id: self.part_id.clone(),
        };
        if let Some(b) = boundary.cleaned() {
            if let Some(id) = self.current_poly_id.take() {
                self.polygons_by_id.insert(id, b.clone());
            }
            if self.in_bounded_by || self.in_lod1 || self.surface_type.is_some() {
                self.boundaries.push(b);
            }
        } else {
            self.current_poly_id = None;
        }
    }

    fn resolve_xlinks(&mut self) {
        if !self.boundaries.is_empty() {
            return;
        }
        for r in &self.xlink_refs {
            if let Some(b) = self.polygons_by_id.get(r) {
                self.boundaries.push(b.clone());
            }
        }
    }

    fn apply_attr(&mut self, tag: &str, value: &str) {
        if self.surface_type.is_some() {
            return;
        }
        // First match wins (same as BIMFabrik `.//bldg:…` XPath).
        match tag {
            "name" if self.gml_name.is_none() => self.gml_name = Some(value.to_string()),
            "function" if self.attributes.function.is_none() => {
                self.attributes.function = Some(value.to_string())
            }
            "measuredHeight" if self.attributes.measured_height.is_none() => {
                if let Ok(h) = value.parse() {
                    self.attributes.measured_height = Some(h);
                }
            }
            "storeysAboveGround" if self.attributes.storeys_above_ground.is_none() => {
                if let Ok(n) = value.parse() {
                    self.attributes.storeys_above_ground = Some(n);
                }
            }
            "roofType" if self.attributes.roof_type.is_none() => {
                self.attributes.roof_type = Some(value.to_string())
            }
            _ => {
                self.attributes.extra.insert(tag.to_string(), value.to_string());
            }
        }
    }

    fn into_building(mut self, bbox: Option<BboxUtm>) -> Option<ParsedBuilding> {
        if self.skip {
            return None;
        }
        self.resolve_xlinks();
        if self.boundaries.is_empty() {
            return None;
        }
        if let Some(bb) = bbox {
            if self.outside_bbox(bb) {
                return None;
            }
        }
        Some(ParsedBuilding {
            id: self.id,
            gml_name: self.gml_name,
            lod: if self.saw_lod3 {
                "LoD3".into()
            } else if self.saw_lod1 {
                "LoD1".into()
            } else {
                "LoD2".into()
            },
            attributes: self.attributes,
            boundaries: self.boundaries,
        })
    }
}

pub fn parse_gml_file(
    path: impl AsRef<Path>,
    building_id_filter: Option<&str>,
    bbox_epsg: Option<BboxUtm>,
    progress: bool,
) -> Result<Vec<ParsedBuilding>, String> {
    let path = path.as_ref();
    let mut reader = Reader::from_file(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    reader.config_mut().trim_text(true);

    let mut buildings = Vec::new();
    let mut current: Option<BuildingAcc> = None;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let qname = e.name();
                let name = local(qname.as_ref());
                if name == b"Building" {
                    let id = attr_local_value(e.attributes(), b"id").unwrap_or_else(|| "unknown".into());
                    let skip = building_id_filter.is_some_and(|f| f != id);
                    current = Some(BuildingAcc::new(id, skip));
                } else if let Some(acc) = current.as_mut() {
                    if name == b"BuildingPart" {
                        acc.part_id = attr_local_value(e.attributes(), b"id");
                    } else if name == b"lod1Solid" {
                        acc.saw_lod1 = true;
                        acc.in_lod1 = true;
                    } else if name == b"lod3MultiSurface" || name == b"lod3Solid" {
                        acc.saw_lod3 = true;
                        acc.skip_geom = name == b"lod3Solid";
                    } else if name == b"lod3TerrainIntersection" {
                        acc.skip_geom = true;
                    } else if name == b"boundedBy" {
                        acc.in_bounded_by = true;
                    } else if SURFACE_TYPES.contains(&name) {
                        acc.surface_type = Some(local_str(name));
                    } else if name == b"exterior" {
                        acc.in_exterior = true;
                    } else if name == b"interior" {
                        acc.in_interior = true;
                    } else if name == b"Polygon" || name == b"Triangle" {
                        acc.current_poly_id = attr_local_value(e.attributes(), b"id");
                    } else if name == b"posList" || name == b"pos" {
                        acc.buf.clear();
                        acc.text_tag = Some("posList".into());
                    } else if matches!(
                        name,
                        b"stringAttribute"
                            | b"intAttribute"
                            | b"doubleAttribute"
                            | b"dateAttribute"
                            | b"uriAttribute"
                            | b"measureAttribute"
                    ) {
                        acc.generic_name = attr_local_value(e.attributes(), b"name");
                    } else if name == b"value" && acc.generic_name.is_some() {
                        acc.text_tag = Some("gen_value".into());
                        acc.buf.clear();
                    } else if matches!(
                        name,
                        b"name"
                            | b"function"
                            | b"measuredHeight"
                            | b"storeysAboveGround"
                            | b"roofType"
                    ) {
                        acc.text_tag = Some(local_str(name));
                        acc.buf.clear();
                    }
                    if !acc.skip_geom {
                        if let Some(href) = attr_local_value(e.attributes(), b"href") {
                            acc.xlink_refs.push(href.trim_start_matches('#').to_string());
                        }
                    }
                }
            }
            Ok(Event::Text(t)) => {
                if let Some(acc) = current.as_mut() {
                    if acc.text_tag.is_some() && !acc.skip {
                        acc.buf.push_str(&t.unescape().unwrap_or_default());
                    }
                }
            }
            Ok(Event::End(e)) => {
                let qname = e.name();
                let name = local(qname.as_ref());
                if name == b"Building" {
                    if let Some(acc) = current.take() {
                        if let Some(b) = acc.into_building(bbox_epsg) {
                            buildings.push(b);
                            if progress && buildings.len() % 25 == 0 {
                                eprint!("\r  parse_gml: {:>5} buildings", buildings.len());
                            }
                            if building_id_filter.is_some() {
                                break;
                            }
                        }
                    }
                } else if let Some(acc) = current.as_mut() {
                    if name == b"BuildingPart" {
                        acc.part_id = None;
                    } else if name == b"lod1Solid" {
                        acc.in_lod1 = false;
                    } else if name == b"lod3Solid" || name == b"lod3TerrainIntersection" {
                        acc.skip_geom = false;
                    } else if name == b"boundedBy" {
                        acc.in_bounded_by = false;
                    } else if SURFACE_TYPES.contains(&name) {
                        acc.surface_type = None;
                    } else if name == b"exterior" {
                        acc.in_exterior = false;
                    } else if name == b"interior" {
                        acc.in_interior = false;
                    } else if name == b"Polygon" || name == b"Triangle" {
                        acc.finish_polygon();
                    } else if name == b"posList" || name == b"pos" {
                        if !acc.skip && !acc.skip_geom {
                            let pts = parse_poslist(&acc.buf);
                            acc.note_points(&pts);
                            if acc.in_interior {
                                acc.interiors.push(pts);
                            } else {
                                acc.exterior = pts;
                            }
                        }
                        acc.buf.clear();
                        acc.text_tag = None;
                    } else if name == b"value" && acc.text_tag.as_deref() == Some("gen_value") {
                        if let Some(key) = acc.generic_name.clone() {
                            let value = acc.buf.trim();
                            if !value.is_empty() && !acc.skip && acc.surface_type.is_none() {
                                acc.attributes.extra.insert(key, value.to_string());
                            }
                        }
                        acc.buf.clear();
                        acc.text_tag = None;
                    } else if matches!(
                        name,
                        b"stringAttribute"
                            | b"intAttribute"
                            | b"doubleAttribute"
                            | b"dateAttribute"
                            | b"uriAttribute"
                            | b"measureAttribute"
                    ) {
                        acc.generic_name = None;
                    } else if acc.text_tag.as_deref() == Some(&local_str(name)) {
                        let value = acc.buf.trim().to_string();
                        if !value.is_empty() && !acc.skip {
                            acc.apply_attr(&local_str(name), &value);
                        }
                        acc.buf.clear();
                        acc.text_tag = None;
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("XML error at {}: {e}", reader.buffer_position())),
            _ => {}
        }
        buf.clear();
    }

    if progress && !buildings.is_empty() {
        eprintln!("\r  parse_gml: {:>5} buildings", buildings.len());
    }
    Ok(buildings)
}
