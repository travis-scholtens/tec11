use crate::vec3::{dist2, Vec3};

/// 3D spatial octree for nearest-neighbor queries (`pointtree.py`).
#[derive(Clone, Debug)]
pub struct PointTree {
    root: PointTreeNode,
}

#[derive(Clone, Debug)]
enum PointTreeNode {
    Leaf(Vec<(Vec3, usize)>),
    Branch {
        medians: Vec3,
        children: Box<[PointTreeNode; 8]>,
    },
}

impl PointTree {
    pub fn new(points: &[(Vec3, usize)]) -> Self {
        assert!(points.len() >= 2, "Must specify at least two points");
        Self {
            root: Self::branch(points.to_vec()),
        }
    }

    fn branch(pts: Vec<(Vec3, usize)>) -> PointTreeNode {
        if pts.len() < 16 {
            return PointTreeNode::Leaf(pts);
        }

        let mut min_v = [f64::INFINITY; 3];
        let mut max_v = [f64::NEG_INFINITY; 3];
        for (v, _) in &pts {
            for c in 0..3 {
                if v[c] < min_v[c] {
                    min_v[c] = v[c];
                }
                if v[c] > max_v[c] {
                    max_v[c] = v[c];
                }
            }
        }
        let medians = [
            0.5 * (min_v[0] + max_v[0]),
            0.5 * (min_v[1] + max_v[1]),
            0.5 * (min_v[2] + max_v[2]),
        ];

        let mut buckets: [Vec<(Vec3, usize)>; 8] = Default::default();
        for item in &pts {
            let ci = Self::child_index(item.0, medians);
            buckets[ci].push(*item);
        }

        if buckets.iter().any(|b| b.len() == pts.len()) {
            return PointTreeNode::Leaf(pts);
        }

        let children = Box::new(buckets.map(Self::branch));
        PointTreeNode::Branch { medians, children }
    }

    #[inline]
    fn child_index(v: Vec3, medians: Vec3) -> usize {
        ((v[0] > medians[0]) as usize)
            | (((v[1] > medians[1]) as usize) << 1)
            | (((v[2] > medians[2]) as usize) << 2)
    }

    pub fn nearest(&self, query: Vec3) -> usize {
        let mut best = (f64::INFINITY, 0usize);
        self.root.nearest_one(query, &mut best);
        best.1
    }

    pub fn nearest_k(&self, query: Vec3, k: usize) -> Vec<usize> {
        let mut best: Vec<(f64, usize)> = Vec::with_capacity(k);
        self.root.nearest_k(query, k, &mut best);
        best.into_iter().map(|(_, idx)| idx).collect()
    }
}

impl PointTreeNode {
    fn nearest_one(&self, v: Vec3, best: &mut (f64, usize)) {
        match self {
            PointTreeNode::Leaf(pts) => {
                for &(tv, idx) in pts {
                    let d2 = dist2(v, tv);
                    if d2 < best.0 {
                        *best = (d2, idx);
                    }
                }
            }
            PointTreeNode::Branch { medians, children } => {
                let ci = PointTree::child_index(v, *medians);
                children[ci].nearest_one(v, best);
                for bi in 0..8 {
                    if bi == ci {
                        continue;
                    }
                    let diff = bi ^ ci;
                    let bound = [
                        if (diff & 1) != 0 { medians[0] } else { v[0] },
                        if (diff & 2) != 0 { medians[1] } else { v[1] },
                        if (diff & 4) != 0 { medians[2] } else { v[2] },
                    ];
                    if dist2(v, bound) < best.0 {
                        children[bi].nearest_one(v, best);
                    }
                }
            }
        }
    }

    fn nearest_k(&self, v: Vec3, k: usize, best: &mut Vec<(f64, usize)>) {
        match self {
            PointTreeNode::Leaf(pts) => {
                for &(tv, idx) in pts {
                    let d2 = dist2(v, tv);
                    if best.len() < k || d2 < best.last().unwrap().0 {
                        best.push((d2, idx));
                        best.sort_by(|a, b| a.0.total_cmp(&b.0));
                        if best.len() > k {
                            best.pop();
                        }
                    }
                }
            }
            PointTreeNode::Branch { medians, children } => {
                let ci = PointTree::child_index(v, *medians);
                children[ci].nearest_k(v, k, best);
                for bi in 0..8 {
                    if bi == ci {
                        continue;
                    }
                    let diff = bi ^ ci;
                    let bound = [
                        if (diff & 1) != 0 { medians[0] } else { v[0] },
                        if (diff & 2) != 0 { medians[1] } else { v[1] },
                        if (diff & 4) != 0 { medians[2] } else { v[2] },
                    ];
                    if best.len() < k || dist2(v, bound) < best.last().unwrap().0 {
                        children[bi].nearest_k(v, k, best);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_point_tree_nearest() {
        let pts = vec![
            ([1.0, 1.0, 0.0], 0),
            ([1.0, 9.0, 0.0], 1),
            ([4.0, 6.0, 0.0], 2),
            ([6.0, 6.0, 0.0], 3),
            ([9.0, 2.0, 0.0], 4),
        ];
        let tree = PointTree::new(&pts);
        assert_eq!(tree.nearest_k([6.0, 4.0, 0.0], 3), vec![3, 2, 4]);
    }
}
