pub mod earth_data;
pub mod koeppen;
pub mod model;
pub mod ocean;

pub use earth_data::EARTH_SIZE6_ELEVATIONS_M;
pub use koeppen::{classify_tile, KoeppenCode, TileClimateClassification};
pub use model::{
    bearing_deg, insolation, nominal_cells_for_spin, temp_range_for_glaciation, wind_direction_deg,
    ClimateModel, DownwindLink, SeasonalSnapshot, TemperatureRange, BREEZE_DISTANCE,
    DEFAULT_SEASONS, DEFAULT_SPIN, DEFAULT_TILT_DEG, MEAN_TEMPRANGE_C,
};
pub use ocean::{solve_ocean_circulation, OceanCirculation};
