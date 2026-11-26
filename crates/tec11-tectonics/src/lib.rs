pub mod movement;
pub mod pangea;
pub mod tectonics;

pub use pangea::PangeaWorld;
pub use tec11_geology::LithosphericGroup;
pub use tectonics::{
    classify_submarine_tile, MantleHotspot, OceanicSubductionArc, SpreadingRidgeArc,
    TectonicsSimulation,
};
