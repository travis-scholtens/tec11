pub mod earth_data;
pub mod model;
pub mod ocean;

pub use earth_data::EARTH_SIZE6_ELEVATIONS_M;
pub use tec11_koeppen::{classify_tile, KoeppenCode, TemperatureRange, TileClimateClassification};
pub use model::{
    bearing_deg, insolation, nominal_cells_for_spin, wind_direction_deg, ClimateModel,
    DownwindLink, SeasonalSnapshot, BREEZE_DISTANCE, DEFAULT_GLACIATION,
    DEFAULT_SEASONS, DEFAULT_SPIN, DEFAULT_TILT_DEG, EARTH_GLACIATION, MEAN_TEMPRANGE_C,
};
pub use ocean::{
    solve_ocean_circulation, solve_seasonal_ocean_circulation,
    solve_seasonal_ocean_circulation_with_geom, OceanCirculation, OceanGridGeometry,
};
