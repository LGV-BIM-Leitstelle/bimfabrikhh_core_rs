use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use crate::types::{BuildingExport, Face, Product, Pset};

use super::guid::ifc_gid;
use super::step::{format_real, step_str, Ids};

/// Values written into `IfcPerson` / `IfcOrganization` / `IfcApplication`.
#[derive(Clone, Debug)]
pub struct OwnerIdentity {
    pub family_name: String,
    pub given_name: String,
    pub organization: String,
    pub application: String,
    pub version: String,
    pub identifier: String,
}

impl Default for OwnerIdentity {
    fn default() -> Self {
        Self {
            family_name: "Salem".into(),
            given_name: "bimfabrikhh_core_rs".into(),
            organization: "bimfabrikhh_core_rs".into(),
            application: "bimfabrikhh_core_rs".into(),
            version: "0.1.0".into(),
            identifier: "bimfabrikhh_core_rs".into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct WriteOptions {
    pub project_name: String,
    pub site_name: String,
    pub epsg: i32,
    pub styles: bool,
    pub owner_history: bool,
    pub owner: OwnerIdentity,
    pub basepoint: Option<(f64, f64)>,
    /// One spatial IfcBuilding; CityGML buildings are products (Python city model).
    pub city_model: bool,
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            project_name: "bimfabrikhh_core_rs".into(),
            site_name: "Site".into(),
            epsg: 25833,
            styles: true,
            owner_history: true,
            owner: OwnerIdentity::default(),
            basepoint: None,
            city_model: false,
        }
    }
}

fn crs_name(epsg: i32) -> (&'static str, &'static str) {
    match epsg {
        25832 => ("ETRS89 / UTM zone 32N", "32"),
        25833 => ("ETRS89 / UTM zone 33N", "33"),
        _ => ("ETRS89 / UTM", "$"),
    }
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
    write_ifc(&mut w, buildings, opts).map_err(|e| format!("write {}: {e}", path.display()))?;
    w.flush().map_err(|e| e.to_string())?;
    Ok(())
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
            let typed = if let Ok(i) = v.parse::<i64>() {
                format!("IFCINTEGER({i})")
            } else if let Ok(f) = v.parse::<f64>() {
                format!("IFCREAL({})", format_real(f))
            } else {
                format!("IFCLABEL({})", step_str(v))
            };
            writeln!(
                w,
                "#{pid}=IFCPROPERTYSINGLEVALUE({key},$,{typed},$);",
                key = step_str(k),
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

#[allow(clippy::too_many_arguments)]
fn write_product_geom<W: Write>(
    w: &mut W,
    id: &mut Ids,
    sub: u32,
    place: u32,
    owner: &str,
    product: &Product,
    pi: usize,
    styles: bool,
) -> std::io::Result<u32> {
    let list = id.next();
    let pts = product
        .vertices
        .iter()
        .map(|p| format!("({},{},{})", format_real(p[0]), format_real(p[1]), format_real(p[2])))
        .collect::<Vec<_>>()
        .join(",");
    writeln!(w, "#{list}=IFCCARTESIANPOINTLIST3D(({pts}));")?;

    let mut face_refs = Vec::new();
    for face in &product.faces {
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
        if let Some(rgb) = product.color {
            let col = id.next();
            let shade = id.next();
            let style = id.next();
            let item = id.next();
            writeln!(
                w,
                "#{col}=IFCCOLOURRGB($,{r},{g},{b});",
                r = format_real(rgb[0]),
                g = format_real(rgb[1]),
                b = format_real(rgb[2])
            )?;
            writeln!(w, "#{shade}=IFCSURFACESTYLESHADING(#{col},0.);")?;
            writeln!(w, "#{style}=IFCSURFACESTYLE('Style',.BOTH.,(#{shade}));")?;
            writeln!(w, "#{item}=IFCSTYLEDITEM(#{faceset},(#{style}),$);")?;
        }
    }

    let shape = id.next();
    let pds = id.next();
    let prod = id.next();
    writeln!(
        w,
        "#{shape}=IFCSHAPEREPRESENTATION(#{sub},'Body','Tessellation',(#{faceset}));"
    )?;
    writeln!(w, "#{pds}=IFCPRODUCTDEFINITIONSHAPE($,$,(#{shape}));")?;
    let ifc_type = product.ifc_type.to_ascii_uppercase();
    writeln!(
        w,
        "#{prod}={ifc_type}({gid},{owner}{name},$,$,#{place},#{pds},$,$);",
        gid = step_str(&ifc_gid(&format!("{}-{pi}", product.name))),
        name = step_str(&product.name),
    )?;
    Ok(prod)
}

fn write_ifc<W: Write>(
    w: &mut W,
    buildings: &[BuildingExport],
    opts: &WriteOptions,
) -> std::io::Result<()> {
    let mut id = Ids::new();
    writeln!(w, "ISO-10303-21;")?;
    writeln!(w, "HEADER;")?;
    writeln!(w, "FILE_DESCRIPTION(('ViewDefinition [Ifc4ReferenceView]'),'2;1');")?;
    writeln!(w, "FILE_NAME('','',(''),(''),'bimfabrikhh_core_rs','bimfabrikhh_core_rs','');")?;
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
    let crs = id.next();
    let mapconv = id.next();
    let rel_ps = id.next();

    let mut owner = String::from("$");
    if opts.owner_history {
        let person = id.next();
        let org = id.next();
        let pao = id.next();
        let app = id.next();
        let oh = id.next();
        let oh_id = &opts.owner;
        writeln!(
            w,
            "#{person}=IFCPERSON($,{family},{given},$,$,$,$,$);",
            family = step_str(&oh_id.family_name),
            given = step_str(&oh_id.given_name),
        )?;
        writeln!(
            w,
            "#{org}=IFCORGANIZATION($,{name},$,$,$);",
            name = step_str(&oh_id.organization),
        )?;
        writeln!(w, "#{pao}=IFCPERSONANDORGANIZATION(#{person},#{org},$);")?;
        writeln!(
            w,
            "#{app}=IFCAPPLICATION(#{org},{version},{full},{ident});",
            version = step_str(&oh_id.version),
            full = step_str(&oh_id.application),
            ident = step_str(&oh_id.identifier),
        )?;
        writeln!(
            w,
            "#{oh}=IFCOWNERHISTORY(#{pao},#{app},$,.ADDED.,$,$,$,0);"
        )?;
        owner = format!("#{oh},");
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
    let (crs_label, zone) = crs_name(opts.epsg);
    let zone_lit = if zone == "$" { "$".into() } else { step_str(zone) };
    writeln!(
        w,
        "#{crs}=IFCPROJECTEDCRS('EPSG:{epsg}',{label},'ETRS89','DHHN2016','Transverse Mercator',{zone},$);",
        epsg = opts.epsg,
        label = step_str(crs_label),
        zone = zone_lit,
    )?;
    writeln!(w, "#{mapconv}=IFCMAPCONVERSION(#{ctx},#{crs},0.,0.,0.,1.,0.,1.);")?;
    writeln!(
        w,
        "#{rel_ps}=IFCRELAGGREGATES({gid},{owner}$,$,#{project_id},(#{site_id}));",
        gid = step_str(&ifc_gid("rel-project-site")),
    )?;

    let total_products: usize = buildings.iter().map(|b| b.products.len()).sum();
    let mut written = 0usize;
    let mut building_ids = Vec::new();
    let mut city_product_refs: Vec<String> = Vec::new();

    let city_bid = if opts.city_model {
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
        let bid = if let Some(shared) = city_bid {
            shared
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
            bid
        };

        let mut product_refs = Vec::new();
        for (pi, product) in bldg.products.iter().enumerate() {
            let prod = write_product_geom(w, &mut id, sub, place, &owner, product, pi, opts.styles)?;
            write_psets(w, &mut id, &owner, prod, &product.psets, &product.name)?;
            product_refs.push(format!("#{prod}"));
            written += 1;
            if written.is_multiple_of(50) || written == total_products {
                eprint!("\r  write_ifc: {written:>5}/{total_products} products");
            }
        }
        if opts.city_model {
            city_product_refs.extend(product_refs);
        } else if !product_refs.is_empty() {
            let rel = id.next();
            writeln!(
                w,
                "#{rel}=IFCRELCONTAINEDINSPATIALSTRUCTURE({gid},{owner}$,$,({items}),#{bid});",
                gid = step_str(&ifc_gid(&format!("rel-contain-{}", bldg.id))),
                items = product_refs.join(","),
            )?;
        }
    }

    if let Some(bid) = city_bid {
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

    if let Some((x, y)) = opts.basepoint {
        let bp = write_basepoint(w, &mut id, sub, place, &owner, x, y)?;
        let rel = id.next();
        writeln!(
            w,
            "#{rel}=IFCRELCONTAINEDINSPATIALSTRUCTURE({gid},{owner}$,$,(#{bp}),#{site_id});",
            gid = step_str(&ifc_gid("rel-basepoint")),
        )?;
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

    if total_products > 0 {
        eprintln!();
    }
    writeln!(w, "ENDSEC;")?;
    writeln!(w, "END-ISO-10303-21;")?;
    Ok(())
}

fn write_basepoint<W: Write>(
    w: &mut W,
    id: &mut Ids,
    sub: u32,
    place: u32,
    owner: &str,
    x: f64,
    y: f64,
) -> std::io::Result<u32> {
    let s = 4.0;
    let pts = [
        [x, y, 0.0],
        [x + s, y, 0.0],
        [x + s, y + s, 0.0],
        [x, y + s, 0.0],
        [x + s * 0.5, y + s * 0.5, s],
    ];
    let list = id.next();
    let body = pts
        .iter()
        .map(|p| format!("({},{},{})", format_real(p[0]), format_real(p[1]), format_real(p[2])))
        .collect::<Vec<_>>()
        .join(",");
    writeln!(w, "#{list}=IFCCARTESIANPOINTLIST3D(({body}));")?;
    let mut faces = Vec::new();
    let face_idx: &[&[i32]] = &[&[1, 2, 3, 4], &[1, 2, 5], &[2, 3, 5], &[3, 4, 5], &[4, 1, 5]];
    for idx in face_idx {
        let fid = id.next();
        let coords = idx.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",");
        writeln!(w, "#{fid}=IFCINDEXEDPOLYGONALFACE(({coords}));")?;
        faces.push(format!("#{fid}"));
    }
    let faceset = id.next();
    let shape = id.next();
    let pds = id.next();
    let prod = id.next();
    writeln!(w, "#{faceset}=IFCPOLYGONALFACESET(#{list},$,({}),$);", faces.join(","))?;
    writeln!(w, "#{shape}=IFCSHAPEREPRESENTATION(#{sub},'Body','Tessellation',(#{faceset}));")?;
    writeln!(w, "#{pds}=IFCPRODUCTDEFINITIONSHAPE($,$,(#{shape}));")?;
    writeln!(
        w,
        "#{prod}=IFCBUILDINGELEMENTPROXY({gid},{owner}'Nullpunktobjekt',$,$,#{place},#{pds},$,$);",
        gid = step_str(&ifc_gid("basepoint")),
    )?;
    Ok(prod)
}
