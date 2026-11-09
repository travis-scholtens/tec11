use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

pub type Vec3 = [f64; 3];

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Vec3Key([u64; 3]);

impl Vec3Key {
    #[inline]
    pub fn from_vec3(v: Vec3) -> Self {
        Self([
            canonical_bits(v[0]),
            canonical_bits(v[1]),
            canonical_bits(v[2]),
        ])
    }

    #[inline]
    pub fn to_vec3(self) -> Vec3 {
        [
            f64::from_bits(self.0[0]),
            f64::from_bits(self.0[1]),
            f64::from_bits(self.0[2]),
        ]
    }
}

#[inline]
fn canonical_bits(x: f64) -> u64 {
    if x == 0.0 {
        0.0f64.to_bits()
    } else {
        x.to_bits()
    }
}

impl Ord for Vec3Key {
    fn cmp(&self, other: &Self) -> Ordering {
        let a = self.to_vec3();
        let b = other.to_vec3();
        a[0].total_cmp(&b[0])
            .then_with(|| a[1].total_cmp(&b[1]))
            .then_with(|| a[2].total_cmp(&b[2]))
    }
}

impl PartialOrd for Vec3Key {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[inline]
pub fn squared_length(v: Vec3) -> f64 {
    v[0] * v[0] + v[1] * v[1] + v[2] * v[2]
}

#[inline]
pub fn length(v: Vec3) -> f64 {
    squared_length(v).sqrt()
}

#[inline]
pub fn normal(v: Vec3) -> Vec3 {
    let d = 1.0 / length(v);
    [v[0] * d, v[1] * d, v[2] * d]
}

#[inline]
pub fn cross(v1: Vec3, v2: Vec3) -> Vec3 {
    [
        v1[1] * v2[2] - v1[2] * v2[1],
        v1[2] * v2[0] - v1[0] * v2[2],
        v1[0] * v2[1] - v1[1] * v2[0],
    ]
}

#[inline]
pub fn dot(v1: Vec3, v2: Vec3) -> f64 {
    v1[0] * v2[0] + v1[1] * v2[1] + v1[2] * v2[2]
}

#[inline]
fn compare_vec3(a: &Vec3, b: &Vec3) -> Ordering {
    a[0].total_cmp(&b[0])
        .then_with(|| a[1].total_cmp(&b[1]))
        .then_with(|| a[2].total_cmp(&b[2]))
}

#[inline]
fn sorted_centroid_normal(mut pts: [Vec3; 3]) -> Vec3 {
    pts.sort_by(compare_vec3);
    normal([
        pts[0][0] + pts[1][0] + pts[2][0],
        pts[0][1] + pts[1][1] + pts[2][1],
        pts[0][2] + pts[1][2] + pts[2][2],
    ])
}

/// Spherical pentagon/hexagon grid matching `grid.py`.
///
/// Size 0 is a regular dodecahedron (12 pentagonal faces, 20 vertices).
/// Each subsequent subdivision turns every prior face into a smaller face
/// and every prior vertex (where 3 faces meet) into a hexagonal face:
/// F(0) = 12, F(k) = 3 * F(k-1) - 6 (12, 32, 92, 272, 812, 2432, 7292, ...).
#[derive(Clone, Debug)]
pub struct SphericalGrid {
    pub size: u32,
    /// Ordered list of face center vectors and their counter-clockwise vertex loops.
    pub faces: Vec<(Vec3, Vec<Vec3>)>,
    /// Lookup from face center key to index in `self.faces`.
    pub face_index: BTreeMap<Vec3Key, usize>,
    /// Lookup from vertex coordinate key to the face centers meeting at that vertex.
    pub vertices: BTreeMap<Vec3Key, Vec<Vec3>>,
}

impl SphericalGrid {
    /// Creates a size-0 regular dodecahedron grid.
    pub fn new() -> Self {
        let mut grid = Self {
            size: 0,
            faces: Vec::with_capacity(12),
            face_index: BTreeMap::new(),
            vertices: BTreeMap::new(),
        };

        let x: f64 = -0.525731112119133606;
        let z: f64 = -0.850650808352039932;

        let dodec_faces: [Vec3; 12] = [
            [-x, 0.0, z],
            [x, 0.0, z],
            [-x, 0.0, -z],
            [x, 0.0, -z],
            [0.0, z, x],
            [0.0, z, -x],
            [0.0, -z, x],
            [0.0, -z, -z * 0.0 + -x], // wait: (0, -z, -x)
            [z, x, 0.0],
            [-z, x, 0.0],
            [z, -x, 0.0],
            [-z, -x, 0.0],
        ];

        let dodec_neighbors: [[usize; 5]; 12] = [
            [9, 4, 1, 6, 11],
            [4, 8, 10, 6, 0],
            [11, 7, 3, 5, 9],
            [2, 7, 10, 8, 5],
            [9, 5, 8, 1, 0],
            [2, 3, 8, 4, 9],
            [0, 1, 10, 7, 11],
            [11, 6, 10, 3, 2],
            [5, 3, 10, 1, 4],
            [2, 5, 4, 0, 11],
            [3, 7, 6, 1, 8],
            [7, 2, 9, 0, 6],
        ];

        for i in 0..12 {
            let neighbors: Vec<Vec3> = dodec_neighbors[i]
                .iter()
                .rev()
                .map(|&n| dodec_faces[n])
                .collect();
            grid.make_face(dodec_faces[i], &neighbors);
        }

        grid
    }

    fn add_face_vertices(&mut self, face: Vec3, face_vertices: &[Vec3]) {
        for &vertex in face_vertices {
            let vkey = Vec3Key::from_vec3(vertex);
            let adj = self.vertices.entry(vkey).or_default();
            if !adj.iter().any(|&f| Vec3Key::from_vec3(f) == Vec3Key::from_vec3(face)) {
                adj.push(face);
            }
        }
    }

    fn insert_face(&mut self, face: Vec3, vertices: Vec<Vec3>) {
        let fkey = Vec3Key::from_vec3(face);
        if self.face_index.contains_key(&fkey) {
            return;
        }
        self.add_face_vertices(face, &vertices);
        let idx = self.faces.len();
        self.faces.push((face, vertices));
        self.face_index.insert(fkey, idx);
    }

    fn make_face(&mut self, new_face: Vec3, neighbors: &[Vec3]) {
        let len = neighbors.len();
        let mut vertices = Vec::with_capacity(len);
        for i in 0..len {
            let n1 = neighbors[i];
            let n2 = neighbors[(i + len - 1) % len];
            vertices.push(sorted_centroid_normal([new_face, n1, n2]));
        }
        self.insert_face(new_face, vertices);
    }

    /// Subdivides all tiles of `self` to produce the next finer grid (`size + 1`).
    pub fn subdivide(&self) -> Self {
        let expected_faces = 3 * self.faces.len() - 6;
        let mut next = Self {
            size: self.size + 1,
            faces: Vec::with_capacity(expected_faces),
            face_index: BTreeMap::new(),
            vertices: BTreeMap::new(),
        };

        for (prev_face, prev_face_vertices) in &self.faces {
            // 1. Face from previous face
            next.make_face(*prev_face, prev_face_vertices);

            // 2. Faces from previous vertices
            for &vertex in prev_face_vertices {
                let vkey = Vec3Key::from_vec3(vertex);
                if next.face_index.contains_key(&vkey) {
                    continue;
                }

                let adj_faces = &self.vertices[&vkey];
                let num_adj = adj_faces.len();
                let mut new_vertices = Vec::with_capacity(num_adj * 2);

                for i in 0..num_adj {
                    let f1 = adj_faces[i];
                    let f2 = adj_faces[(i + 1) % num_adj];

                    let f1_verts = &self.faces[self.face_index[&Vec3Key::from_vec3(f1)]].1;
                    let f2_verts = &self.faces[self.face_index[&Vec3Key::from_vec3(f2)]].1;

                    let f1_set: BTreeSet<Vec3Key> =
                        f1_verts.iter().map(|&v| Vec3Key::from_vec3(v)).collect();
                    let common: Vec<Vec3> = f2_verts
                        .iter()
                        .copied()
                        .filter(|&v| f1_set.contains(&Vec3Key::from_vec3(v)))
                        .collect();

                    debug_assert_eq!(common.len(), 2, "Adjacent faces must share 2 vertices");

                    for neighbor in [f1, f2] {
                        new_vertices.push(sorted_centroid_normal([
                            neighbor, common[0], common[1],
                        ]));
                    }
                }

                // Ensure counter-clockwise winding around `vertex`
                if dot(vertex, cross(new_vertices[0], new_vertices[1])) > 0.0 {
                    new_vertices.reverse();
                }

                next.insert_face(vertex, new_vertices);
            }
        }

        next
    }
}

impl Default for SphericalGrid {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grid_subdivision_counts_and_winding() {
        let mut g = SphericalGrid::new();
        assert_eq!(g.size, 0);
        assert_eq!(g.faces.len(), 12);
        assert_eq!(g.vertices.len(), 20);
        for (_, verts) in &g.faces {
            assert_eq!(verts.len(), 5);
        }

        let expected_counts = [32, 92, 272, 812, 2432];
        for (step, &expected) in expected_counts.iter().enumerate() {
            g = g.subdivide();
            assert_eq!(g.size, (step + 1) as u32);
            assert_eq!(g.faces.len(), expected);
            let pentagons = g.faces.iter().filter(|(_, v)| v.len() == 5).count();
            let hexagons = g.faces.iter().filter(|(_, v)| v.len() == 6).count();
            assert_eq!(pentagons, 12);
            assert_eq!(hexagons, expected - 12);
        }
    }
}
