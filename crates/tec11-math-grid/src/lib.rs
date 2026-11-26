pub mod geometry;
pub mod grid;
pub mod noise;
pub mod point_tree;
pub mod terrain;

pub use tec11_rng as rng;
pub use tec11_vec3 as vec3;

pub use geometry::{GreatCircle, GreatCircleArc, SphericalPolygon};
pub use grid::SphericalGrid;
pub use noise::{simplex3, snoise3};
pub use point_tree::PointTree;
pub use tec11_rng::Pcg64Mcg;
pub use terrain::{
    classify_fine_parentage, compute_fine_land_elevation, compute_subgrid_volcanic_peak_km,
    route_coarse_rivers_on_fine_grid, DisplayFace, DisplayFaceSource, DisplayMesh, FineParentage,
    FullSphereFineGrid, SubdividedTerrainGrid,
};
pub use tec11_vec3::{
    cross, dist2, dot, great_circle_distance, length, normal, rotate, squared_length, Vec3, Vec3Key,
};
