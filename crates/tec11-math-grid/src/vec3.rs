use std::cmp::Ordering;

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
pub fn dist2(v1: Vec3, v2: Vec3) -> f64 {
    let dx = v1[0] - v2[0];
    let dy = v1[1] - v2[1];
    let dz = v1[2] - v2[2];
    dx * dx + dy * dy + dz * dz
}

/// Great-circle angular distance (in radians) between two unit vectors on the sphere.
/// Matches `Tile.distance` in `tile.py`.
#[inline]
pub fn great_circle_distance(a: Vec3, b: Vec3) -> f64 {
    let c = cross(a, b);
    length(c).atan2(dot(a, b))
}

/// Rotate vector `p` about `axis` by `theta` radians (`movemethod.py:9-24`).
pub fn rotate(p: Vec3, axis: Vec3, theta: f64) -> Vec3 {
    let l2 = squared_length(axis);
    if l2 == 0.0 {
        return p;
    }
    let l = l2.sqrt();
    let cos_t = theta.cos();
    let sin_t = theta.sin();
    let p_dot_a = dot(p, axis);

    [
        (axis[0] * p_dot_a
            + (p[0] * (axis[1] * axis[1] + axis[2] * axis[2])
                + axis[0] * (-axis[1] * p[1] - axis[2] * p[2]))
                * cos_t
            + l * (-axis[2] * p[1] + axis[1] * p[2]) * sin_t)
            / l2,
        (axis[1] * p_dot_a
            + (p[1] * (axis[0] * axis[0] + axis[2] * axis[2])
                + axis[1] * (-axis[0] * p[0] - axis[2] * p[2]))
                * cos_t
            + l * (axis[2] * p[0] - axis[0] * p[2]) * sin_t)
            / l2,
        (axis[2] * p_dot_a
            + (p[2] * (axis[0] * axis[0] + axis[1] * axis[1])
                + axis[2] * (-axis[0] * p[0] - axis[1] * p[1]))
                * cos_t
            + l * (-axis[1] * p[0] + axis[0] * p[1]) * sin_t)
            / l2,
    ]
}
