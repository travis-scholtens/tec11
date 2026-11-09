use tec11_math_grid::{normal, SphericalGrid, Vec3};

#[inline]
pub fn rotate_axes(v: Vec3) -> [f32; 3] {
    [-v[1] as f32, v[2] as f32, v[0] as f32]
}

pub fn build_mesh_positions(grid: &SphericalGrid) -> Vec<f32> {
    let total_tris: usize = grid.faces.iter().map(|(_, vs)| vs.len()).sum();
    let mut positions = Vec::with_capacity(total_tris * 9);

    for (center, vs) in &grid.faces {
        let t = rotate_axes(*center);
        let len = vs.len();
        for i in 0..len {
            let v_prev = rotate_axes(vs[(i + len - 1) % len]);
            let v_curr = rotate_axes(vs[i]);
            positions.extend_from_slice(&t);
            positions.extend_from_slice(&v_prev);
            positions.extend_from_slice(&v_curr);
        }
    }
    positions
}

pub fn build_mesh_normals(grid: &SphericalGrid) -> Vec<f32> {
    let total_tris: usize = grid.faces.iter().map(|(_, vs)| vs.len()).sum();
    let mut normals = Vec::with_capacity(total_tris * 9);

    for (center, vs) in &grid.faces {
        let n = rotate_axes(normal(*center));
        for _ in 0..vs.len() {
            normals.extend_from_slice(&n);
            normals.extend_from_slice(&n);
            normals.extend_from_slice(&n);
        }
    }
    normals
}

pub fn build_vertex_tile_ids(grid: &SphericalGrid) -> Vec<u32> {
    let total_tris: usize = grid.faces.iter().map(|(_, vs)| vs.len()).sum();
    let mut ids = Vec::with_capacity(total_tris * 3);

    for (tile_idx, (_, vs)) in grid.faces.iter().enumerate() {
        let tid = tile_idx as u32;
        for _ in 0..(vs.len() * 3) {
            ids.push(tid);
        }
    }
    ids
}
