//! Nullpunktobjekt geometry, the shape of BIMFabrikHH `create_basepoint_quad`:
//! a star of two square pyramids with an "N" and a north arrow recessed into the
//! top face and the arrow into the bottom face, each recess filled by a thin
//! prism. Local metres, origin at the basepoint.

/// "N" outline relative to the edge length. BIMFabrikHH `get_n_base_coordinates`
/// without its repeated closing point.
const N_OUTLINE: [[f64; 2]; 10] = [
    [-0.05, 0.35],
    [-0.033, 0.35],
    [-0.033, 0.425],
    [0.033, 0.35],
    [0.05, 0.35],
    [0.05, 0.45],
    [0.033, 0.45],
    [0.033, 0.375],
    [-0.033, 0.45],
    [-0.05, 0.45],
];

/// North arrow relative to the edge length. BIMFabrikHH `get_arrow_base_coordinates`.
const ARROW_OUTLINE: [[f64; 2]; 3] = [[-0.3, -0.3], [0.3, -0.3], [0.0, 0.30015]];

/// Prism thickness relative to the edge length. BIMFabrikHH `EXTRUSION_HEIGHT_FACTOR`.
const MARK_DEPTH_FACTOR: f64 = 0.05;

/// One face as 0-based point indices: outer loop plus holes.
#[derive(Clone, Debug)]
pub struct MeshFace {
    pub outer: Vec<usize>,
    pub voids: Vec<Vec<usize>>,
}

impl MeshFace {
    fn simple(outer: Vec<usize>) -> Self {
        Self { outer, voids: Vec::new() }
    }
}

#[derive(Clone, Debug)]
pub struct Mesh {
    pub points: Vec<[f64; 3]>,
    pub faces: Vec<MeshFace>,
}

pub struct BasepointGeometry {
    /// The star, with the recesses as holes in its top and bottom face.
    pub body: Mesh,
    /// "N" on top, arrow on top, arrow on the bottom; flush with the faces.
    pub marks: Vec<Mesh>,
}

pub fn basepoint_geometry(size: f64) -> BasepointGeometry {
    let depth = MARK_DEPTH_FACTOR * size;
    BasepointGeometry {
        body: body(size),
        marks: vec![
            prism(&N_OUTLINE, size, size - depth, size),
            prism(&ARROW_OUTLINE, size, size - depth, size),
            prism(&ARROW_OUTLINE, size, -size, -size + depth),
        ],
    }
}

fn body(size: f64) -> Mesh {
    let h = 0.5 * size;
    let mut points = vec![
        [-h, -h, size],
        [h, -h, size],
        [h, h, size],
        [-h, h, size],
        [-h, -h, -size],
        [h, -h, -size],
        [h, h, -size],
        [-h, h, -size],
        [0.0, 0.0, 0.0],
    ];
    // Holes run against the outer loop, as in BIMFabrikHH `_create_pyramid_faces`.
    let mut hole = |outline: &[[f64; 2]], z: f64| -> Vec<usize> {
        let start = points.len();
        points.extend(outline.iter().map(|[x, y]| [x * size, y * size, z]));
        (start..points.len()).rev().collect()
    };
    let n_top = hole(&N_OUTLINE, size);
    let arrow_top = hole(&ARROW_OUTLINE, size);
    let arrow_bottom = hole(&ARROW_OUTLINE, -size);

    let mut faces: Vec<MeshFace> = [
        [0, 8, 1],
        [1, 8, 2],
        [2, 8, 3],
        [3, 8, 0],
        [5, 8, 4],
        [6, 8, 5],
        [7, 8, 6],
        [4, 8, 7],
    ]
    .iter()
    .map(|f| MeshFace::simple(f.to_vec()))
    .collect();
    faces.push(MeshFace { outer: vec![0, 1, 2, 3], voids: vec![n_top, arrow_top] });
    faces.push(MeshFace { outer: vec![4, 5, 6, 7], voids: vec![arrow_bottom] });
    Mesh { points, faces }
}

/// Closed prism over `outline` from `z0` to `z1`, faces pointing outwards.
fn prism(outline: &[[f64; 2]], size: f64, z0: f64, z1: f64) -> Mesh {
    let mut ring: Vec<[f64; 2]> = outline.iter().map(|[x, y]| [x * size, y * size]).collect();
    if signed_area(&ring) < 0.0 {
        ring.reverse();
    }
    let n = ring.len();
    let points = ring
        .iter()
        .map(|&[x, y]| [x, y, z0])
        .chain(ring.iter().map(|&[x, y]| [x, y, z1]))
        .collect();
    let mut faces = vec![MeshFace::simple((n..2 * n).collect()), MeshFace::simple((0..n).rev().collect())];
    for i in 0..n {
        let j = (i + 1) % n;
        faces.push(MeshFace::simple(vec![i, j, n + j, n + i]));
    }
    Mesh { points, faces }
}

fn signed_area(ring: &[[f64; 2]]) -> f64 {
    let n = ring.len();
    (0..n)
        .map(|i| {
            let ([x0, y0], [x1, y1]) = (ring[i], ring[(i + 1) % n]);
            x0 * y1 - x1 * y0
        })
        .sum::<f64>()
        / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Signed volume of a closed mesh; positive when every face points outwards.
    fn volume(mesh: &Mesh) -> f64 {
        let p = |i: usize| mesh.points[i];
        let mut v = 0.0;
        for face in &mesh.faces {
            let o = p(face.outer[0]);
            for k in 1..face.outer.len() - 1 {
                let (a, b) = (p(face.outer[k]), p(face.outer[k + 1]));
                v += o[0] * (a[1] * b[2] - a[2] * b[1]) - o[1] * (a[0] * b[2] - a[2] * b[0])
                    + o[2] * (a[0] * b[1] - a[1] * b[0]);
            }
        }
        v / 6.0
    }

    #[test]
    fn body_has_the_recesses_as_holes() {
        let g = basepoint_geometry(8.0);
        assert_eq!(g.body.points.len(), 9 + N_OUTLINE.len() + 2 * ARROW_OUTLINE.len());
        assert_eq!(g.body.faces.len(), 10);
        let top = &g.body.faces[8];
        assert_eq!(top.outer, vec![0, 1, 2, 3]);
        assert_eq!(top.voids.iter().map(Vec::len).collect::<Vec<_>>(), vec![10, 3]);
        assert_eq!(g.body.faces[9].voids.len(), 1);
        assert!(top.voids.iter().flatten().all(|&i| g.body.points[i][2] == 8.0));
    }

    #[test]
    fn marks_are_flush_outward_prisms() {
        let size = 5.0;
        let g = basepoint_geometry(size);
        let depth = MARK_DEPTH_FACTOR * size;
        assert_eq!(g.marks.len(), 3);
        for (mark, outline) in g.marks.iter().zip([&N_OUTLINE[..], &ARROW_OUTLINE, &ARROW_OUTLINE]) {
            let scaled: Vec<[f64; 2]> = outline.iter().map(|[x, y]| [x * size, y * size]).collect();
            let expected = signed_area(&scaled).abs() * depth;
            assert!((volume(mark) - expected).abs() < 1e-9, "{} vs {expected}", volume(mark));
        }
        let z = |m: &Mesh| {
            let zs: Vec<f64> = m.points.iter().map(|p| p[2]).collect();
            (zs.iter().cloned().fold(f64::MAX, f64::min), zs.iter().cloned().fold(f64::MIN, f64::max))
        };
        assert_eq!(z(&g.marks[0]), (size - depth, size));
        assert_eq!(z(&g.marks[2]), (-size, -size + depth));
    }

    #[test]
    fn geometry_scales_with_size() {
        let (one, eight) = (basepoint_geometry(1.0), basepoint_geometry(8.0));
        for (a, b) in one.body.points.iter().zip(&eight.body.points) {
            for k in 0..3 {
                assert!((a[k] * 8.0 - b[k]).abs() < 1e-12);
            }
        }
    }
}
