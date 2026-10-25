//! Coordinate system a CityGML / CityJSON source declares (`srsName` /
//! `metadata.referenceSystem`). There is no PROJ here: only the names are
//! read, to check and complete the caller's `crs`. Nothing is transformed.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use serde_json::Value;

use crate::ifc::ProjectedCrs;

/// Bytes read from the start of a CityGML file to find the first `srsName`.
const SRS_NAME_SCAN_BYTES: u64 = 65536;

const ADV_HORIZONTAL: [(&str, u32); 2] = [("ETRS89_UTM32", 25832), ("ETRS89_UTM33", 25833)];
const ADV_VERTICAL: [(&str, u32); 2] = [("DE_DHHN2016_NH", 7837), ("DE_DHHN92_NH", 5783)];
/// Compound codes split into (horizontal, vertical). Other single codes are
/// taken as horizontal only.
const COMPOUND: [(u32, u32, u32); 2] = [(5555, 25832, 5783), (5556, 25833, 5783)];

/// EPSG codes named by a source; `None` for parts the name does not give.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeclaredCrs {
    pub srs_name: String,
    pub horizontal_epsg: Option<u32>,
    pub vertical_epsg: Option<u32>,
}

impl DeclaredCrs {
    /// Understands AdV URNs (`urn:adv:crs:ETRS89_UTM32*DE_DHHN2016_NH`), OGC
    /// compound URNs (`urn:ogc:def:crs,crs:EPSG:6.12:25833,crs:EPSG:6.12:7837`)
    /// and single EPSG codes (`EPSG:25832`, `urn:ogc:def:crs:EPSG::25832`,
    /// `https://www.opengis.net/def/crs/EPSG/0/25832`).
    pub fn from_srs_name(srs_name: &str) -> Self {
        let lookup = |table: &[(&str, u32)], key: &str| table.iter().find(|(k, _)| *k == key).map(|(_, v)| *v);
        let (horizontal_epsg, vertical_epsg) = if let Some(rest) = srs_name.strip_prefix("urn:adv:crs:") {
            let (horizontal, vertical) = rest.split_once('*').unwrap_or((rest, ""));
            (lookup(&ADV_HORIZONTAL, horizontal), lookup(&ADV_VERTICAL, vertical))
        } else {
            match epsg_codes(srs_name)[..] {
                [horizontal, vertical, ..] => (Some(horizontal), Some(vertical)),
                [code] => match COMPOUND.iter().find(|(c, _, _)| *c == code) {
                    Some(&(_, horizontal, vertical)) => (Some(horizontal), Some(vertical)),
                    None => (Some(code), None),
                },
                [] => (None, None),
            }
        };
        DeclaredCrs { srs_name: srs_name.to_string(), horizontal_epsg, vertical_epsg }
    }

    fn codes(&self) -> (Option<u32>, Option<u32>) {
        (self.horizontal_epsg, self.vertical_epsg)
    }
}

/// Every EPSG code in `text`, in order: `EPSG:c`, `EPSG::c`, `EPSG:v:c`, `EPSG/v/c`.
fn epsg_codes(text: &str) -> Vec<u32> {
    let leading = |s: &str, ok: fn(char) -> bool| s.find(|c: char| !ok(c)).unwrap_or(s.len());
    let mut codes = Vec::new();
    for (i, _) in text.match_indices("EPSG") {
        let rest = &text[i + 4..];
        let Some(sep) = rest.chars().next().filter(|c| *c == ':' || *c == '/') else {
            continue;
        };
        let body = &rest[1..];
        let first = &body[..leading(body, |c| c.is_ascii_digit() || c == '.')];
        let code = match body[first.len()..].strip_prefix(sep) {
            Some(after_version) => &after_version[..leading(after_version, |c| c.is_ascii_digit())],
            None => first,
        };
        if let Ok(code) = code.parse() {
            codes.push(code);
        }
    }
    codes
}

/// The single EPSG code of a name like `EPSG:25832`, else `None`.
fn single_epsg(name: &str) -> Option<u32> {
    match epsg_codes(name)[..] {
        [code] => Some(code),
        _ => None,
    }
}

/// CRS of the first `srsName` in a CityGML file; `None` when it has none.
pub fn gml_declared_crs(path: impl AsRef<Path>) -> Result<Option<DeclaredCrs>, String> {
    let path = path.as_ref();
    let mut head = Vec::new();
    File::open(path)
        .and_then(|f| f.take(SRS_NAME_SCAN_BYTES).read_to_end(&mut head))
        .map_err(|e| format!("open {}: {e}", path.display()))?;
    let head = String::from_utf8_lossy(&head);
    Ok(head
        .split_once("srsName=\"")
        .and_then(|(_, rest)| rest.split_once('"'))
        .map(|(srs_name, _)| DeclaredCrs::from_srs_name(srs_name)))
}

/// CRS of `metadata.referenceSystem` in a parsed CityJSON document; `None` when it has none.
pub fn cityjson_declared_crs(root: &Value) -> Option<DeclaredCrs> {
    root.pointer("/metadata/referenceSystem")
        .and_then(Value::as_str)
        .map(DeclaredCrs::from_srs_name)
}

/// The one CRS all sources declare; sources without one are skipped.
pub fn common_declared_crs(
    declared: impl IntoIterator<Item = Option<DeclaredCrs>>,
) -> Result<Option<DeclaredCrs>, String> {
    let mut common: Option<DeclaredCrs> = None;
    for crs in declared.into_iter().flatten() {
        match &common {
            Some(first) if first.codes() != crs.codes() => {
                return Err(format!(
                    "the sources declare different CRS: {:?} and {:?}",
                    first.srs_name, crs.srs_name
                ));
            }
            Some(_) => {}
            None => common = Some(crs),
        }
    }
    Ok(common)
}

/// Check `crs` against what the sources declare and fill a missing vertical datum.
///
/// The horizontal code is only compared when the coordinates are written as
/// parsed (`transformed == false`); after a transform `crs` is the output CRS.
/// Heights are never transformed, so the vertical datum is always compared.
/// Names that are not `EPSG:<code>` are not compared.
pub fn reconcile_crs(
    crs: Option<ProjectedCrs>,
    declared: Option<&DeclaredCrs>,
    transformed: bool,
) -> Result<Option<ProjectedCrs>, String> {
    let (Some(mut crs), Some(declared)) = (crs.clone(), declared) else {
        return Ok(crs);
    };
    if !transformed {
        if let (Some(code), Some(source)) = (single_epsg(&crs.name), declared.horizontal_epsg) {
            if code != source {
                return Err(format!(
                    "crs {} contradicts the source, which declares EPSG:{source} ({:?}); pass transform= to reproject",
                    crs.name, declared.srs_name
                ));
            }
        }
    }
    match (&crs.vertical_datum, declared.vertical_epsg) {
        (Some(datum), Some(source)) => {
            if single_epsg(datum).is_some_and(|code| code != source) {
                return Err(format!(
                    "crs vertical_datum {datum} contradicts the source, whose heights are EPSG:{source} ({:?})",
                    declared.srs_name
                ));
            }
        }
        (None, Some(source)) => crs.vertical_datum = Some(format!("EPSG:{source}")),
        (_, None) => {}
    }
    Ok(Some(crs))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(srs_name: &str) -> (Option<u32>, Option<u32>) {
        DeclaredCrs::from_srs_name(srs_name).codes()
    }

    fn utm32(vertical_datum: Option<&str>) -> ProjectedCrs {
        ProjectedCrs {
            name: "EPSG:25832".into(),
            description: None,
            geodetic_datum: "ETRS89".into(),
            vertical_datum: vertical_datum.map(Into::into),
            map_projection: None,
            map_zone: None,
        }
    }

    #[test]
    fn srs_names_give_horizontal_and_vertical_codes() {
        assert_eq!(codes("urn:adv:crs:ETRS89_UTM32*DE_DHHN2016_NH"), (Some(25832), Some(7837)));
        assert_eq!(codes("urn:adv:crs:ETRS89_UTM33"), (Some(25833), None));
        assert_eq!(codes("urn:ogc:def:crs,crs:EPSG:6.12:25833,crs:EPSG:6.12:7837"), (Some(25833), Some(7837)));
        assert_eq!(codes("urn:ogc:def:crs:EPSG::5555"), (Some(25832), Some(5783)));
        assert_eq!(codes("EPSG:25832"), (Some(25832), None));
        assert_eq!(codes("https://www.opengis.net/def/crs/EPSG/0/25832"), (Some(25832), None));
        assert_eq!(codes("local"), (None, None));
    }

    #[test]
    fn cityjson_reference_system_is_read() {
        let doc = serde_json::json!({"type": "CityJSON", "metadata": {"referenceSystem": "https://www.opengis.net/def/crs/EPSG/0/5555"}});
        assert_eq!(cityjson_declared_crs(&doc).unwrap().codes(), (Some(25832), Some(5783)));
        assert_eq!(cityjson_declared_crs(&serde_json::json!({"type": "CityJSON"})), None);
    }

    #[test]
    fn sources_with_different_crs_are_rejected() {
        let a = Some(DeclaredCrs::from_srs_name("EPSG:25832"));
        let b = Some(DeclaredCrs::from_srs_name("EPSG:25833"));
        assert!(common_declared_crs([a.clone(), None, a.clone()]).unwrap().is_some());
        assert!(common_declared_crs([a, b]).is_err());
    }

    #[test]
    fn reconcile_fills_missing_vertical_datum() {
        let declared = DeclaredCrs::from_srs_name("urn:adv:crs:ETRS89_UTM32*DE_DHHN2016_NH");
        let crs = reconcile_crs(Some(utm32(None)), Some(&declared), false).unwrap().unwrap();
        assert_eq!(crs.vertical_datum.as_deref(), Some("EPSG:7837"));
        assert_eq!(reconcile_crs(None, Some(&declared), false).unwrap(), None);
    }

    #[test]
    fn reconcile_rejects_contradicting_crs() {
        let declared = DeclaredCrs::from_srs_name("urn:adv:crs:ETRS89_UTM33*DE_DHHN2016_NH");
        assert!(reconcile_crs(Some(utm32(None)), Some(&declared), false).is_err());
        assert!(reconcile_crs(Some(utm32(None)), Some(&declared), true).is_ok());
        let dhhn92 = DeclaredCrs::from_srs_name("urn:adv:crs:ETRS89_UTM32*DE_DHHN92_NH");
        assert!(reconcile_crs(Some(utm32(Some("EPSG:7837"))), Some(&dhhn92), true).is_err());
        assert!(reconcile_crs(Some(utm32(Some("DHHN2016"))), Some(&dhhn92), false).is_ok());
    }
}
