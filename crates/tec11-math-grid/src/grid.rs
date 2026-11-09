use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use crate::vec3::{cross, dot, normal, Vec3, Vec3Key};

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
            [0.0, -z, -x],
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

    /// Creates a spherical grid subdivided to `target_size`.
    pub fn with_size(target_size: u32) -> Self {
        let mut grid = Self::new();
        while grid.size < target_size {
            grid = grid.subdivide();
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
            next.make_face(*prev_face, prev_face_vertices);

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

                if dot(vertex, cross(new_vertices[0], new_vertices[1])) > 0.0 {
                    new_vertices.reverse();
                }

                next.insert_face(vertex, new_vertices);
            }
        }

        next
    }

    /// Computes the neighbor index list for every face (`hexadjacency.py`).
    pub fn adjacency(&self) -> Vec<Vec<usize>> {
        let mut adj = Vec::with_capacity(self.faces.len());
        for (idx, (_, verts)) in self.faces.iter().enumerate() {
            let mut neighbors = Vec::with_capacity(verts.len());
            for &v in verts {
                let vkey = Vec3Key::from_vec3(v);
                if let Some(adj_faces) = self.vertices.get(&vkey) {
                    for &af in adj_faces {
                        if let Some(&n_idx) = self.face_index.get(&Vec3Key::from_vec3(af)) {
                            if n_idx != idx && !neighbors.contains(&n_idx) {
                                neighbors.push(n_idx);
                            }
                        }
                    }
                }
            }
            adj.push(neighbors);
        }
        adj
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
