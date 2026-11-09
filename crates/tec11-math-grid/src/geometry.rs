use std::f64::consts::PI;

use crate::vec3::{cross, dot, great_circle_distance, length, normal, squared_length, Vec3};

/// Great circle defined by two points on the unit sphere (`greatcircle.py`).
#[derive(Clone, Copy, Debug)]
pub struct GreatCircle {
    pub start: Vec3,
    pub end: Vec3,
    pub plane: Vec3,
}

impl GreatCircle {
    pub fn new(start: Vec3, end: Vec3) -> Self {
        Self {
            start,
            end,
            plane: cross(start, end),
        }
    }

    pub fn intersects(&self, other: &GreatCircle) -> Option<[Vec3; 2]> {
        if self.plane == [0.0, 0.0, 0.0] || other.plane == [0.0, 0.0, 0.0] {
            return None;
        }
        let line = cross(self.plane, other.plane);
        let len2 = squared_length(line);
        if len2 == 0.0 {
            return None;
        }
        let d = 1.0 / len2.sqrt();
        let p1 = [line[0] * d, line[1] * d, line[2] * d];
        let p2 = [-p1[0], -p1[1], -p1[2]];
        Some([p1, p2])
    }
}

/// Directed arc along a great circle (`greatcirclearc.py`).
#[derive(Clone, Copy, Debug)]
pub struct GreatCircleArc {
    pub start: Vec3,
    pub end: Vec3,
    pub length: f64,
    pub circle: GreatCircle,
}

impl GreatCircleArc {
    pub fn new(start: Vec3, end: Vec3) -> Self {
        let d = dot(start, end).clamp(-1.0, 1.0);
        Self {
            start,
            end,
            length: d.acos(),
            circle: GreatCircle::new(start, end),
        }
    }

    pub fn contains(&self, v: Vec3) -> bool {
        let dth = if dot(cross(self.start, v), self.circle.plane) < 0.0 {
            PI
        } else {
            0.0
        };
        let angle = dot(v, self.start).clamp(-1.0, 1.0).acos();
        angle + dth < self.length
    }

    pub fn intersects(&self, other: &GreatCircleArc) -> bool {
        if let Some(intersections) = self.circle.intersects(&other.circle) {
            for intersection in intersections {
                if self.contains(intersection) && other.contains(intersection) {
                    return true;
                }
            }
        }
        false
    }

    /// Shortest angular distance (radians) on the unit sphere from `v` to this arc segment.
    pub fn distance_to_point(&self, v: Vec3) -> f64 {
        let plane_len = length(self.circle.plane);
        if plane_len > 1e-12 {
            let n = [
                self.circle.plane[0] / plane_len,
                self.circle.plane[1] / plane_len,
                self.circle.plane[2] / plane_len,
            ];
            let vn = dot(v, n);
            let proj = [v[0] - vn * n[0], v[1] - vn * n[1], v[2] - vn * n[2]];
            if length(proj) > 1e-12 {
                let p = normal(proj);
                if self.contains(p) {
                    return great_circle_distance(v, p);
                }
            }
        }
        great_circle_distance(v, self.start).min(great_circle_distance(v, self.end))
    }
}

/// Spherical polygon on S^2 with Jordan curve ray-casting (`sphericalpolygon.py`).
#[derive(Clone, Debug)]
pub struct SphericalPolygon {
    arcs: Vec<GreatCircleArc>,
    external_vector: Vec3,
}

impl SphericalPolygon {
    pub fn new(vectors: &[Vec3]) -> Self {
        let len = vectors.len();
        let mut arcs = Vec::with_capacity(len);
        for i in 0..len {
            let v1 = vectors[(i + len - 1) % len];
            let v2 = vectors[i];
            let (s, e) = if v1[0] * v2[1] - v1[1] * v2[0] < 0.0 {
                (v2, v1)
            } else {
                (v1, v2)
            };
            arcs.push(GreatCircleArc::new(s, e));
        }

        let mut sum = [0.0, 0.0, 0.0];
        for v in vectors {
            sum[0] += v[0];
            sum[1] += v[1];
            sum[2] += v[2];
        }
        let n = len as f64;
        let external_vector = normal([-sum[0] / n, -sum[1] / n, -sum[2] / n]);

        Self {
            arcs,
            external_vector,
        }
    }

    pub fn contains(&self, vector: Vec3) -> bool {
        let ray = GreatCircleArc::new(vector, self.external_vector);
        let count = self.arcs.iter().filter(|a| ray.intersects(a)).count();
        (count % 2) == 1
    }

    /// Shortest angular distance (radians) from `v` to any boundary arc of this polygon.
    pub fn distance_to_boundary(&self, v: Vec3) -> f64 {
        self.arcs
            .iter()
            .map(|a| a.distance_to_point(v))
            .fold(f64::INFINITY, f64::min)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_great_circle_arc_intersections() {
        let arc = GreatCircleArc::new(
            [0.29210493879571187, -0.8867057671346513, -0.35836794954530027],
            [-0.21032302, 0.93088621, 0.29868896],
        );
        let others = [
            GreatCircleArc::new(
                [0.07826526, -0.8631922, -0.49877228],
                [0.47401636, -0.79000325, -0.38884876],
            ),
            GreatCircleArc::new(
                [-0.02999477, -0.96742597, -0.25137087],
                [0.06420691, -0.9818318, -0.17856034],
            ),
        ];
        let count = others.iter().filter(|o| arc.intersects(o)).count();
        assert_eq!(count, 1);
    }

    fn latlon_vec(lat_deg: f64, lon_deg: f64) -> Vec3 {
        let lat = lat_deg * PI / 180.0;
        let lon = lon_deg * PI / 180.0;
        let cos_lat = lat.cos();
        [cos_lat * lon.cos(), cos_lat * lon.sin(), lat.sin()]
    }

    #[test]
    fn test_spherical_polygon_contains() {
        let us = SphericalPolygon::new(&[
            latlon_vec(48.60, -124.06),
            latlon_vec(47.96, -89.17),
            latlon_vec(41.50, -81.71),
            latlon_vec(47.22, -68.73),
            latlon_vec(44.56, -67.10),
            latlon_vec(30.26, -82.53),
            latlon_vec(32.47, -117.20),
        ]);
        assert!(us.contains(latlon_vec(38.62, -98.62))); // Kansas
        assert!(!us.contains(latlon_vec(53.49, -113.64))); // Alberta
    }
}
