use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::color::{BASEPOINT_ARROW_RGB, BASEPOINT_RGB, BASEPOINT_SIZE, DEFAULT_RGB};
use crate::types::{BuildingExport, Face, Product, ProductPart, PropValue, Pset};

use super::basepoint::{basepoint_geometry, Mesh};
use super::guid::ifc_gid;
use super::step::{format_real, step_str, Ids};

/// CAD layer of the Nullpunktobjekt body. Same as BIMFabrikHH `create_basepoint_quad`.
pub const BASEPOINT_LAYER: &str = "_Projektnullpunkt";
/// CAD layer of the Nullpunktobjekt "N" and north arrow (BIMFabrikHH `ARROW_LAYER`).
pub const BASEPOINT_ARROW_LAYER: &str = "_Projektnullpunkt_pfeil";

/// Values written into `IfcPerson` / `IfcOrganization` / `IfcApplication`.
#[derive(Clone, Debug)]
pub struct OwnerIdentity {
    pub family_name: String,
    pub given_name: String,
    /// FILE_NAME author (Autor/in) and IfcOrganization.
    pub organization: String,
    pub application: String,
    pub version: String,
    pub identifier: String,
    /// FILE_NAME preprocessor_version (toolbox).
    pub preprocessor: String,
    /// FILE_NAME originating_system (the application).
    pub originating_system: String,
    /// FILE_NAME authorization (often empty).
    pub authorization: String,
}

impl Default for OwnerIdentity {
    fn default() -> Self {
        Self {
            family_name: "Salem".into(),
            given_name: "Ahmed".into(),
            organization: "BIM-Leitstelle, LGV Hamburg".into(),
            application: "BIMFabrikHH".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            identifier: "BIMFabrikHH".into(),
            preprocessor: concat!("bimfabrikhh_core_rs ", env!("CARGO_PKG_VERSION")).into(),
            originating_system: "BIMFabrikHH".into(),
            authorization: String::new(),
        }
    }
}

/// `IfcProjectedCRS` attributes. There is no PROJ here, so the caller fills
/// them (BIMFabrikHH_core `CoordinateSystem`, `bimfabrikhh_core_rs.crs`).
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectedCrs {
    /// e.g. `EPSG:25832`
    pub name: String,
    pub description: Option<String>,
    pub geodetic_datum: String,
    pub vertical_datum: Option<String>,
    pub map_projection: Option<String>,
    pub map_zone: Option<String>,
}

#[derive(Clone, Debug)]
pub struct WriteOptions {
    pub project_name: String,
    pub site_name: String,
    /// Map CRS of the coordinates. `None` writes no `IfcProjectedCRS` /
    /// `IfcMapConversion`: the coordinates have no known CRS.
    pub crs: Option<ProjectedCrs>,
    pub styles: bool,
    /// One `IfcPresentationLayerWithStyle` named `_BIM_Stadtmodell` on every product mesh.
    pub cad_layer: bool,
    pub owner_history: bool,
    pub owner: OwnerIdentity,
    pub basepoint: Option<(f64, f64)>,
    /// Nullpunktobjekt edge length in metres. Only used with `basepoint`.
    pub basepoint_size: f64,
    /// Property sets on the Nullpunktobjekt, written as given.
    /// Empty writes none. Only used with `basepoint`.
    pub basepoint_psets: Vec<Pset>,
    /// One spatial IfcBuilding; CityGML buildings are products (Python city model).
    pub city_model: bool,
    /// Hang products on `IfcSite` (no `IfcBuilding`). DGM / BIMFabrik terrain.
    pub contain_in_site: bool,
    pub progress: bool,
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            project_name: "bimfabrikhh_core_rs".into(),
            site_name: "Site".into(),
            crs: None,
            styles: true,
            cad_layer: false,
            owner_history: true,
            owner: OwnerIdentity::default(),
            basepoint: None,
            basepoint_size: BASEPOINT_SIZE,
            basepoint_psets: Vec::new(),
            city_model: false,
            contain_in_site: false,
            progress: false,
        }
    }
}

fn step_opt(value: &Option<String>) -> String {
    value.as_deref().map(step_str).unwrap_or_else(|| "$".into())
}

pub fn write_ifc_file(
    buildings: &[BuildingExport],
    path: impl AsRef<Path>,
    opts: &WriteOptions,
) -> Result<(), String> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let file = File::create(path).map_err(|e| format!("create {}: {e}", path.display()))?;
    let mut w = BufWriter::new(file);
    let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("out.ifc");
    write_ifc(&mut w, buildings, opts, file_name).map_err(|e| format!("write {}: {e}", path.display()))?;
    w.flush().map_err(|e| e.to_string())?;
    Ok(())
}

fn step_value(v: &PropValue) -> String {
    match v {
        PropValue::Label(s) => format!("IFCLABEL({})", step_str(s)),
        PropValue::Text(s) => format!("IFCTEXT({})", step_str(s)),
        PropValue::Integer(i) => format!("IFCINTEGER({i})"),
        PropValue::Real(f) => format!("IFCREAL({})", format_real(*f)),
        PropValue::Boolean(b) => format!("IFCBOOLEAN({})", if *b { ".T." } else { ".F." }),
        PropValue::LengthMeasure(f) => format!("IFCLENGTHMEASURE({})", format_real(*f)),
        PropValue::AreaMeasure(f) => format!("IFCAREAMEASURE({})", format_real(*f)),
    }
}

fn write_psets<W: Write>(
    w: &mut W,
    id: &mut Ids,
    owner: &str,
    target: u32,
    psets: &[Pset],
    seed: &str,
) -> std::io::Result<()> {
    for pset in psets {
        if pset.is_empty() {
            continue;
        }
        let pset_name = &pset.name;
        let mut prop_ids = Vec::new();
        for (k, v) in &pset.properties {
            let pid = id.next();
            writeln!(
                w,
                "#{pid}=IFCPROPERTYSINGLEVALUE({key},$,{typed},$);",
                key = step_str(k),
                typed = step_value(v),
            )?;
            prop_ids.push(format!("#{pid}"));
        }
        let pset_id = id.next();
        let rel_id = id.next();
        writeln!(
            w,
            "#{pset_id}=IFCPROPERTYSET({gid},{owner}{name},$,({props}));",
            gid = step_str(&ifc_gid(&format!("pset-{seed}-{pset_name}"))),
            name = step_str(pset_name),
            props = prop_ids.join(","),
        )?;
        writeln!(
            w,
            "#{rel_id}=IFCRELDEFINESBYPROPERTIES({gid},{owner}$,$,(#{target}),#{pset_id});",
            gid = step_str(&ifc_gid(&format!("relpset-{seed}-{pset_name}"))),
        )?;
    }
    Ok(())
}

/// Same chain as the BIMFabrik ifcfactory writer: `IfcStyledItem` -> `IfcSurfaceStyle`
/// -> `IfcSurfaceStyleShading` (BricsCAD shows it).
fn write_item_style<W: Write>(
    w: &mut W,
    id: &mut Ids,
    faceset: u32,
    rgb: [f64; 3],
) -> std::io::Result<()> {
    let col = id.next();
    let shading = id.next();
    let style = id.next();
    let item = id.next();
    writeln!(
        w,
        "#{col}=IFCCOLOURRGB($,{r},{g},{b});",
        r = format_real(rgb[0]),
        g = format_real(rgb[1]),
        b = format_real(rgb[2]),
    )?;
    writeln!(w, "#{shading}=IFCSURFACESTYLESHADING(#{col},0.);")?;
    writeln!(w, "#{style}=IFCSURFACESTYLE('Style',.BOTH.,(#{shading}));")?;
    writeln!(w, "#{item}=IFCSTYLEDITEM(#{faceset},(#{style}),$);")?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn write_product_geom<W: Write>(
    w: &mut W,
    id: &mut Ids,
    sub: u32,
    site_place: u32,
    dir_z: u32,
    dir_x: u32,
    owner: &str,
    product: &Product,
    pi: usize,
    styles: bool,
) -> std::io::Result<(u32, Vec<(u32, Option<String>, Option<[f64; 3]>)>)> {
    let primary = ProductPart {
        vertices: product.vertices.clone(),
        faces: product.faces.clone(),
        color: product.color,
        layer: product.layer.clone(),
    };
    let mut parts = vec![primary];
    parts.extend(product.extra.iter().cloned());

    let mut item_refs = Vec::new();
    let mut layer_hits = Vec::new();
    for part in &parts {
        let faceset = write_mesh_part(w, id, part, styles)?;
        item_refs.push(format!("#{faceset}"));
        layer_hits.push((faceset, part.layer.clone(), part.color));
    }

    let place = if let Some([x, y, z]) = product.placement {
        let pt = id.next();
        let axis = id.next();
        let loc = id.next();
        writeln!(
            w,
            "#{pt}=IFCCARTESIANPOINT(({},{},{}));",
            format_real(x),
            format_real(y),
            format_real(z),
        )?;
        writeln!(w, "#{axis}=IFCAXIS2PLACEMENT3D(#{pt},#{dir_z},#{dir_x});")?;
        writeln!(w, "#{loc}=IFCLOCALPLACEMENT(#{site_place},#{axis});")?;
        loc
    } else {
        site_place
    };

    let shape = id.next();
    let pds = id.next();
    let prod = id.next();
    writeln!(
        w,
        "#{shape}=IFCSHAPEREPRESENTATION(#{sub},'Body','Tessellation',({items}));",
        items = item_refs.join(","),
    )?;
    writeln!(w, "#{pds}=IFCPRODUCTDEFINITIONSHAPE($,$,(#{shape}));")?;
    let ifc_type = product.ifc_type.to_ascii_uppercase();
    writeln!(
        w,
        "#{prod}={ifc_type}({gid},{owner}{name},$,$,#{place},#{pds},$,$);",
        gid = step_str(&ifc_gid(&format!("{}-{pi}", product.name))),
        name = step_str(&product.name),
    )?;
    Ok((prod, layer_hits))
}

fn write_mesh_part<W: Write>(
    w: &mut W,
    id: &mut Ids,
    part: &ProductPart,
    styles: bool,
) -> std::io::Result<u32> {
    let list = id.next();
    let pts = part
        .vertices
        .iter()
        .map(|p| format!("({},{},{})", format_real(p[0]), format_real(p[1]), format_real(p[2])))
        .collect::<Vec<_>>()
        .join(",");
    writeln!(w, "#{list}=IFCCARTESIANPOINTLIST3D(({pts}));")?;

    let mut face_refs = Vec::new();
    for face in &part.faces {
        let fid = id.next();
        match face {
            Face::Simple(idx) => {
                let coords = idx.iter().map(|i| (i + 1).to_string()).collect::<Vec<_>>().join(",");
                writeln!(w, "#{fid}=IFCINDEXEDPOLYGONALFACE(({coords}));")?;
            }
            Face::WithVoids { outer, inners } => {
                let outer_s = outer.iter().map(|i| (i + 1).to_string()).collect::<Vec<_>>().join(",");
                let inners_s = inners
                    .iter()
                    .map(|ring| {
                        format!(
                            "({})",
                            ring.iter().map(|i| (i + 1).to_string()).collect::<Vec<_>>().join(",")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                writeln!(
                    w,
                    "#{fid}=IFCINDEXEDPOLYGONALFACEWITHVOIDS(({outer_s}),({inners_s}));"
                )?;
            }
        }
        face_refs.push(format!("#{fid}"));
    }

    let faceset = id.next();
    writeln!(
        w,
        "#{faceset}=IFCPOLYGONALFACESET(#{list},$,({}),$);",
        face_refs.join(",")
    )?;

    if styles {
        if let Some(rgb) = part.color {
            write_item_style(w, id, faceset, rgb)?;
        }
    }
    Ok(faceset)
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// UTC Y-M-D h:m:s from Unix seconds (Howard Hinnant civil_from_days).
fn utc_ymdhms(secs: i64) -> (i32, u32, u32, u32, u32, u32) {
    let z = secs.div_euclid(86400) + 719468;
    let era = z.div_euclid(146097);
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i32 + era as i32 * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    let tod = secs.rem_euclid(86400) as u32;
    (y, m, d, tod / 3600, (tod % 3600) / 60, tod % 60)
}

fn file_name_stamp(secs: i64) -> String {
    let (y, m, d, h, min, s) = utc_ymdhms(secs);
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{min:02}:{s:02}")
}

fn write_ifc<W: Write>(
    w: &mut W,
    buildings: &[BuildingExport],
    opts: &WriteOptions,
    file_name: &str,
) -> std::io::Result<()> {
    let mut id = Ids::new();
    writeln!(w, "ISO-10303-21;")?;
    writeln!(w, "HEADER;")?;
    let created = unix_now();
    let ohn = &opts.owner;
    let author = ohn.organization.clone();
    // The buildingSMART Validation Service accepts ReferenceView_V1.2 for IFC4;
    // CoordinationView is IFC2x3 only and Ifc4ReferenceView fails HEADER_03.
    writeln!(w, "FILE_DESCRIPTION(('ViewDefinition [ReferenceView_V1.2]'),'2;1');")?;
    writeln!(
        w,
        "FILE_NAME({name},{stamp},({author}),({org}),{pre},{sys},{auth});",
        name = step_str(file_name),
        stamp = step_str(&file_name_stamp(created)),
        author = step_str(&author),
        org = step_str(&ohn.organization),
        pre = step_str(&ohn.preprocessor),
        sys = step_str(&ohn.originating_system),
        auth = step_str(&ohn.authorization),
    )?;
    writeln!(w, "FILE_SCHEMA(('IFC4'));")?;
    writeln!(w, "ENDSEC;")?;
    writeln!(w, "DATA;")?;

    let unit_len = id.next();
    let unit_area = id.next();
    let units = id.next();
    let origin = id.next();
    let dir_z = id.next();
    let dir_x = id.next();
    let axis = id.next();
    let ctx = id.next();
    let ctx_plan = id.next();
    let origin2 = id.next();
    let dir2 = id.next();
    let axis2 = id.next();
    let sub = id.next();
    let project_id = id.next();
    let site_id = id.next();
    let place = id.next();
    let rel_ps = id.next();

    let mut owner = String::from("$");
    if opts.owner_history {
        let person = id.next();
        let org = id.next();
        let pao = id.next();
        let app = id.next();
        let ohn = id.next();
        writeln!(
            w,
            "#{person}=IFCPERSON($,{family},$,$,$,$,$,$);",
            family = step_str(&opts.owner.organization),
        )?;
        writeln!(
            w,
            "#{org}=IFCORGANIZATION($,{name},$,$,$);",
            name = step_str(&opts.owner.organization),
        )?;
        writeln!(w, "#{pao}=IFCPERSONANDORGANIZATION(#{person},#{org},$);")?;
        writeln!(
            w,
            "#{app}=IFCAPPLICATION(#{org},{version},{full},{ident});",
            version = step_str(&opts.owner.version),
            full = step_str(&opts.owner.application),
            ident = step_str(&opts.owner.identifier),
        )?;
        // IFC4 CORRECTCHANGEACTION: ADDED without LastModifiedDate is invalid.
        writeln!(
            w,
            "#{ohn}=IFCOWNERHISTORY(#{pao},#{app},$,.NOTDEFINED.,$,$,$,{created});"
        )?;
        owner = format!("#{ohn},");
    }

    writeln!(w, "#{unit_len}=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);")?;
    writeln!(w, "#{unit_area}=IFCSIUNIT(*,.AREAUNIT.,$,.SQUARE_METRE.);")?;
    writeln!(w, "#{units}=IFCUNITASSIGNMENT((#{unit_area},#{unit_len}));")?;
    writeln!(w, "#{origin}=IFCCARTESIANPOINT((0.,0.,0.));")?;
    writeln!(w, "#{dir_z}=IFCDIRECTION((0.,0.,1.));")?;
    writeln!(w, "#{dir_x}=IFCDIRECTION((1.,0.,0.));")?;
    writeln!(w, "#{axis}=IFCAXIS2PLACEMENT3D(#{origin},#{dir_z},#{dir_x});")?;
    writeln!(w, "#{origin2}=IFCCARTESIANPOINT((0.,0.));")?;
    writeln!(w, "#{dir2}=IFCDIRECTION((1.,0.));")?;
    writeln!(w, "#{axis2}=IFCAXIS2PLACEMENT2D(#{origin2},#{dir2});")?;
    writeln!(
        w,
        "#{ctx}=IFCGEOMETRICREPRESENTATIONCONTEXT($,'Model',3,1.E-05,#{axis},$);"
    )?;
    writeln!(
        w,
        "#{ctx_plan}=IFCGEOMETRICREPRESENTATIONCONTEXT($,'Plan',2,1.E-05,#{axis2},$);"
    )?;
    writeln!(
        w,
        "#{sub}=IFCGEOMETRICREPRESENTATIONSUBCONTEXT('Body','Model',*,*,*,*,#{ctx},$,.MODEL_VIEW.,$);"
    )?;
    writeln!(
        w,
        "#{project_id}=IFCPROJECT({gid},{owner}{name},$,$,$,$,(#{ctx},#{ctx_plan}),#{units});",
        gid = step_str(&ifc_gid("project")),
        name = step_str(&opts.project_name),
    )?;
    writeln!(w, "#{place}=IFCLOCALPLACEMENT($,#{axis});")?;
    writeln!(
        w,
        "#{site_id}=IFCSITE({gid},{owner}{name},$,$,#{place},$,$,$,$,$,$,$,$);",
        gid = step_str(&ifc_gid("site")),
        name = step_str(&opts.site_name),
    )?;
    if let Some(projected) = &opts.crs {
        let crs = id.next();
        let mapconv = id.next();
        writeln!(
            w,
            "#{crs}=IFCPROJECTEDCRS({name},{description},{geodetic},{vertical},{projection},{zone},$);",
            name = step_str(&projected.name),
            description = step_opt(&projected.description),
            geodetic = step_str(&projected.geodetic_datum),
            vertical = step_opt(&projected.vertical_datum),
            projection = step_opt(&projected.map_projection),
            zone = step_opt(&projected.map_zone),
        )?;
        writeln!(w, "#{mapconv}=IFCMAPCONVERSION(#{ctx},#{crs},0.,0.,0.,1.,0.,1.);")?;
    }
    writeln!(
        w,
        "#{rel_ps}=IFCRELAGGREGATES({gid},{owner}$,$,#{project_id},(#{site_id}));",
        gid = step_str(&ifc_gid("rel-project-site")),
    )?;

    let total_products: usize = buildings.iter().map(|b| b.products.len()).sum();
    let mut written = 0usize;
    let mut building_ids = Vec::new();
    let mut city_product_refs: Vec<String> = Vec::new();
    let mut layer_items: BTreeMap<String, (Vec<String>, [f64; 3])> = BTreeMap::new();

    let city_bid = if opts.contain_in_site {
        None
    } else if opts.city_model {
        let bid = id.next();
        building_ids.push(bid);
        writeln!(
            w,
            "#{bid}=IFCBUILDING({gid},{owner}{name},$,$,#{place},$,$,$,$,$,$);",
            gid = step_str(&ifc_gid("bldg-city")),
            name = step_str(&opts.project_name),
        )?;
        Some(bid)
    } else {
        None
    };

    for bldg in buildings {
        let bid = if opts.contain_in_site {
            None
        } else if let Some(shared) = city_bid {
            Some(shared)
        } else {
            let bid = id.next();
            building_ids.push(bid);
            writeln!(
                w,
                "#{bid}=IFCBUILDING({gid},{owner}{name},$,$,#{place},$,$,$,$,$,$);",
                gid = step_str(&ifc_gid(&format!("bldg-{}", bldg.id))),
                name = step_str(&bldg.display_name),
            )?;
            write_psets(w, &mut id, &owner, bid, &bldg.psets, &bldg.id)?;
            Some(bid)
        };

        let mut product_refs = Vec::new();
        for (pi, product) in bldg.products.iter().enumerate() {
            let (prod, hits) =
                write_product_geom(w, &mut id, sub, place, dir_z, dir_x, &owner, product, pi, opts.styles)?;
            write_psets(w, &mut id, &owner, prod, &product.psets, &product.name)?;
            product_refs.push(format!("#{prod}"));
            if opts.cad_layer {
                for (faceset, layer, color) in hits {
                    let name = layer.unwrap_or_else(|| "_BIM_Stadtmodell".into());
                    let rgb = color.unwrap_or(DEFAULT_RGB);
                    let entry = layer_items.entry(name).or_insert_with(|| (Vec::new(), rgb));
                    entry.0.push(format!("#{faceset}"));
                }
            }
            written += 1;
            if opts.progress && (written.is_multiple_of(50) || written == total_products) {
                eprint!("\r  write_ifc: {written:>5}/{total_products} products");
            }
        }
        if opts.contain_in_site || opts.city_model {
            city_product_refs.extend(product_refs);
        } else if !product_refs.is_empty() {
            let bid = bid.expect("building id when not contain_in_site");
            let rel = id.next();
            writeln!(
                w,
                "#{rel}=IFCRELCONTAINEDINSPATIALSTRUCTURE({gid},{owner}$,$,({items}),#{bid});",
                gid = step_str(&ifc_gid(&format!("rel-contain-{}", bldg.id))),
                items = product_refs.join(","),
            )?;
        }
    }

    if opts.contain_in_site {
        if !city_product_refs.is_empty() {
            let rel = id.next();
            writeln!(
                w,
                "#{rel}=IFCRELCONTAINEDINSPATIALSTRUCTURE({gid},{owner}$,$,({items}),#{site_id});",
                gid = step_str(&ifc_gid("rel-contain-site")),
                items = city_product_refs.join(","),
            )?;
        }
    } else if let Some(bid) = city_bid {
        if !city_product_refs.is_empty() {
            let rel = id.next();
            writeln!(
                w,
                "#{rel}=IFCRELCONTAINEDINSPATIALSTRUCTURE({gid},{owner}$,$,({items}),#{bid});",
                gid = step_str(&ifc_gid("rel-contain-city")),
                items = city_product_refs.join(","),
            )?;
        }
    }

    if let Some(xy) = opts.basepoint {
        let (bp, body, marks) =
            write_basepoint(w, &mut id, sub, place, (dir_z, dir_x), &owner, xy, opts.basepoint_size, opts.styles)?;
        write_psets(w, &mut id, &owner, bp, &opts.basepoint_psets, "basepoint")?;
        // Always layered, independent of `cad_layer`, like the BIMFabrik Python Nullpunktobjekt.
        layer_items
            .entry(BASEPOINT_LAYER.into())
            .or_insert_with(|| (Vec::new(), BASEPOINT_RGB))
            .0
            .push(format!("#{body}"));
        layer_items
            .entry(BASEPOINT_ARROW_LAYER.into())
            .or_insert_with(|| (Vec::new(), BASEPOINT_ARROW_RGB))
            .0
            .extend(marks.iter().map(|m| format!("#{m}")));
        let rel = id.next();
        writeln!(
            w,
            "#{rel}=IFCRELCONTAINEDINSPATIALSTRUCTURE({gid},{owner}$,$,(#{bp}),#{site_id});",
            gid = step_str(&ifc_gid("rel-basepoint")),
        )?;
    }

    if !layer_items.is_empty() {
        for (name, (items, rgb)) in &layer_items {
            if items.is_empty() {
                continue;
            }
            let col = id.next();
            let shade = id.next();
            let style = id.next();
            let layer = id.next();
            let [r, g, b] = *rgb;
            writeln!(
                w,
                "#{col}=IFCCOLOURRGB('LayerColor',{r},{g},{b});",
                r = format_real(r),
                g = format_real(g),
                b = format_real(b),
            )?;
            writeln!(w, "#{shade}=IFCSURFACESTYLESHADING(#{col},0.);")?;
            writeln!(w, "#{style}=IFCSURFACESTYLE('LayerStyle',.POSITIVE.,(#{shade}));")?;
            writeln!(
                w,
                "#{layer}=IFCPRESENTATIONLAYERWITHSTYLE({layer_name},$,({items}),$,.T.,.F.,.F.,(#{style}));",
                layer_name = step_str(name),
                items = items.join(","),
            )?;
        }
    }

    if !building_ids.is_empty() {
        let rel = id.next();
        let items = building_ids.iter().map(|i| format!("#{i}")).collect::<Vec<_>>().join(",");
        writeln!(
            w,
            "#{rel}=IFCRELAGGREGATES({gid},{owner}$,$,#{site_id},({items}));",
            gid = step_str(&ifc_gid("rel-site-buildings")),
        )?;
    }

    if opts.progress && total_products > 0 {
        eprintln!();
    }
    writeln!(w, "ENDSEC;")?;
    writeln!(w, "END-ISO-10303-21;")?;
    Ok(())
}

/// One `IfcPolygonalFaceSet` of `mesh`. Faces with holes become
/// `IfcIndexedPolygonalFaceWithVoids`.
fn write_mesh_faceset<W: Write>(w: &mut W, id: &mut Ids, mesh: &Mesh) -> std::io::Result<u32> {
    let list = id.next();
    let coords = mesh
        .points
        .iter()
        .map(|p| format!("({},{},{})", format_real(p[0]), format_real(p[1]), format_real(p[2])))
        .collect::<Vec<_>>()
        .join(",");
    writeln!(w, "#{list}=IFCCARTESIANPOINTLIST3D(({coords}));")?;
    // IFC indices are 1-based.
    let ring = |r: &[usize]| r.iter().map(|i| (i + 1).to_string()).collect::<Vec<_>>().join(",");
    let mut faces = Vec::with_capacity(mesh.faces.len());
    for face in &mesh.faces {
        let fid = id.next();
        if face.voids.is_empty() {
            writeln!(w, "#{fid}=IFCINDEXEDPOLYGONALFACE(({}));", ring(&face.outer))?;
        } else {
            let voids = face.voids.iter().map(|v| format!("({})", ring(v))).collect::<Vec<_>>().join(",");
            writeln!(w, "#{fid}=IFCINDEXEDPOLYGONALFACEWITHVOIDS(({}),({voids}));", ring(&face.outer))?;
        }
        faces.push(format!("#{fid}"));
    }
    let faceset = id.next();
    writeln!(w, "#{faceset}=IFCPOLYGONALFACESET(#{list},$,({}),$);", faces.join(","))?;
    Ok(faceset)
}

/// The Nullpunktobjekt, shaped like BIMFabrikHH `create_basepoint_quad`:
/// placed at `(x, y, 0)` relative to `site_place`, with local geometry.
/// Returns the product, the body faceset and the "N" / arrow facesets.
#[allow(clippy::too_many_arguments)]
fn write_basepoint<W: Write>(
    w: &mut W,
    id: &mut Ids,
    sub: u32,
    site_place: u32,
    (dir_z, dir_x): (u32, u32),
    owner: &str,
    (x, y): (f64, f64),
    size: f64,
    styles: bool,
) -> std::io::Result<(u32, u32, Vec<u32>)> {
    let geometry = basepoint_geometry(size);
    let (pt, axis, place) = (id.next(), id.next(), id.next());
    writeln!(w, "#{pt}=IFCCARTESIANPOINT(({},{},0.));", format_real(x), format_real(y))?;
    writeln!(w, "#{axis}=IFCAXIS2PLACEMENT3D(#{pt},#{dir_z},#{dir_x});")?;
    writeln!(w, "#{place}=IFCLOCALPLACEMENT(#{site_place},#{axis});")?;
    let body = write_mesh_faceset(w, id, &geometry.body)?;
    if styles {
        write_item_style(w, id, body, BASEPOINT_RGB)?;
    }
    let mut marks = Vec::with_capacity(geometry.marks.len());
    for mark in &geometry.marks {
        let faceset = write_mesh_faceset(w, id, mark)?;
        if styles {
            write_item_style(w, id, faceset, BASEPOINT_ARROW_RGB)?;
        }
        marks.push(faceset);
    }
    let shape = id.next();
    let pds = id.next();
    let prod = id.next();
    let items = std::iter::once(body)
        .chain(marks.iter().copied())
        .map(|i| format!("#{i}"))
        .collect::<Vec<_>>()
        .join(",");
    writeln!(w, "#{shape}=IFCSHAPEREPRESENTATION(#{sub},'Body','Tessellation',({items}));")?;
    writeln!(w, "#{pds}=IFCPRODUCTDEFINITIONSHAPE($,$,(#{shape}));")?;
    writeln!(
        w,
        "#{prod}=IFCBUILDINGELEMENTPROXY({gid},{owner}'Nullpunktobjekt',$,$,#{place},#{pds},$,$);",
        gid = step_str(&ifc_gid("basepoint")),
    )?;
    Ok((prod, body, marks))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_styles_use_surface_style_shading() {
        let text = basepoint_text(Vec::new());
        assert!(text.contains("IFCSURFACESTYLESHADING("));
        assert!(!text.contains("IFCSURFACESTYLERENDERING("));
        assert!(!text.contains("IFCPRESENTATIONSTYLEASSIGNMENT("));
    }

    #[test]
    fn header_declares_the_ifc4_reference_view() {
        assert!(step_text(None).contains("FILE_DESCRIPTION(('ViewDefinition [ReferenceView_V1.2]'),'2;1');"));
    }

    #[test]
    fn unix_epoch_formats_as_iso() {
        assert_eq!(file_name_stamp(0), "1970-01-01T00:00:00");
        assert_eq!(file_name_stamp(86400), "1970-01-02T00:00:00");
        assert_eq!(file_name_stamp(1_700_000_000), "2023-11-14T22:13:20");
    }

    fn step_text(crs: Option<ProjectedCrs>) -> String {
        let opts = WriteOptions { crs, ..WriteOptions::default() };
        let mut out = Vec::new();
        write_ifc(&mut out, &[], &opts, "test.ifc").unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn projected_crs_writes_caller_fields() {
        let text = step_text(Some(ProjectedCrs {
            name: "EPSG:10285".into(),
            description: Some("ETRS89/DREF91/2016 / 3-degree Gauss-Kruger zone 3".into()),
            geodetic_datum: "ETRS89/DREF91/2016".into(),
            vertical_datum: Some("EPSG:7837".into()),
            map_projection: Some("Transverse Mercator".into()),
            map_zone: Some("3".into()),
        }));
        assert!(text.contains(
            "IFCPROJECTEDCRS('EPSG:10285','ETRS89/DREF91/2016 / 3-degree Gauss-Kruger zone 3',\
             'ETRS89/DREF91/2016','EPSG:7837','Transverse Mercator','3',$);"
        ));
        assert!(text.contains("IFCMAPCONVERSION("));
    }

    #[test]
    fn projected_crs_writes_missing_fields_as_unset() {
        let text = step_text(Some(ProjectedCrs {
            name: "EPSG:25832".into(),
            description: None,
            geodetic_datum: "ETRS89".into(),
            vertical_datum: None,
            map_projection: None,
            map_zone: None,
        }));
        assert!(text.contains("IFCPROJECTEDCRS('EPSG:25832',$,'ETRS89',$,$,$,$);"));
    }

    #[test]
    fn no_crs_writes_no_georeferencing() {
        let text = step_text(None);
        assert!(!text.contains("IFCPROJECTEDCRS"));
        assert!(!text.contains("IFCMAPCONVERSION"));
        assert!(text.contains("IFCSITE("));
    }

    fn basepoint_text(basepoint_psets: Vec<Pset>) -> String {
        let opts = WriteOptions {
            basepoint: Some((565000.0, 5934000.0)),
            basepoint_psets,
            ..WriteOptions::default()
        };
        let mut out = Vec::new();
        write_ifc(&mut out, &[], &opts, "test.ifc").unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn basepoint_psets_keep_their_types() {
        let text = basepoint_text(vec![Pset::new(
            "Pset_Modellinformation",
            vec![
                ("_ArtTeilmodell".into(), PropValue::label("Gelaende")),
                ("_Projektnummer".into(), PropValue::label("12345")),
            ],
        )]);
        assert!(text.contains("IFCPROPERTYSINGLEVALUE('_ArtTeilmodell',$,IFCLABEL('Gelaende'),$);"));
        assert!(text.contains("IFCPROPERTYSINGLEVALUE('_Projektnummer',$,IFCLABEL('12345'),$);"));
        assert!(text.contains("'Pset_Modellinformation',$,("));
    }

    #[test]
    fn values_are_written_with_the_declared_type() {
        let cases = [
            (PropValue::Text("lang".into()), "IFCTEXT('lang')"),
            (PropValue::Integer(300), "IFCINTEGER(300)"),
            (PropValue::Real(3.0), "IFCREAL(3.)"),
            (PropValue::Boolean(false), "IFCBOOLEAN(.F.)"),
            (PropValue::LengthMeasure(3.7), "IFCLENGTHMEASURE(3.7)"),
            (PropValue::AreaMeasure(12.5), "IFCAREAMEASURE(12.5)"),
        ];
        for (value, step) in cases {
            assert_eq!(step_value(&value), step);
        }
    }

    #[test]
    fn basepoint_without_psets_writes_none() {
        let text = basepoint_text(Vec::new());
        assert!(text.contains("'Nullpunktobjekt'"));
        assert!(!text.contains("IFCPROPERTYSET("));
    }

    #[test]
    fn basepoint_is_on_its_layer_without_cad_layer() {
        let text = basepoint_text(Vec::new());
        assert!(text.contains("IFCPRESENTATIONLAYERWITHSTYLE('_Projektnullpunkt',$,(#"));
    }

    #[test]
    fn basepoint_has_n_and_arrow_on_their_layer() {
        let text = basepoint_text(Vec::new());
        assert_eq!(text.matches("IFCINDEXEDPOLYGONALFACEWITHVOIDS(").count(), 2);
        assert_eq!(text.matches("IFCPOLYGONALFACESET(").count(), 4);
        let arrow_layer = text
            .lines()
            .find(|l| l.contains("IFCPRESENTATIONLAYERWITHSTYLE('_Projektnullpunkt_pfeil'"))
            .expect("arrow layer");
        assert_eq!(arrow_layer.matches('#').count(), 1 + 3 + 1, "{arrow_layer}");
        let grey = format_real(64.0 / 255.0);
        assert!(text.contains(&format!("IFCCOLOURRGB($,{grey},{grey},{grey});")));
    }

    #[test]
    fn basepoint_size_scales_the_star() {
        let opts = WriteOptions {
            basepoint: Some((0.0, 0.0)),
            basepoint_size: 1.0,
            ..WriteOptions::default()
        };
        let mut out = Vec::new();
        write_ifc(&mut out, &[], &opts, "test.ifc").unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("IFCCARTESIANPOINTLIST3D(((-0.5,-0.5,1.),"));
    }

    #[test]
    fn basepoint_is_placed_at_its_xy_with_local_geometry() {
        let opts = WriteOptions {
            basepoint: Some((565000.0, 5933500.0)),
            basepoint_size: 1.0,
            ..WriteOptions::default()
        };
        let mut out = Vec::new();
        write_ifc(&mut out, &[], &opts, "test.ifc").unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("IFCCARTESIANPOINT((565000.,5933500.,0.));"));
        assert!(text.contains("IFCCARTESIANPOINTLIST3D(((-0.5,-0.5,1.),"));
        assert!(!text.contains("IFCCARTESIANPOINTLIST3D(((565"));
    }
}
