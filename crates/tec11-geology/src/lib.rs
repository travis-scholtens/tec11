pub mod movement;
pub mod pangea;
pub mod profile;
pub mod rivers;
pub mod rock;
pub mod tectonics;
pub mod tile;

pub use tec11_cosmos as sun_sky;
pub use tec11_sky_view as sky_vantage;

/// Convert `&[TileGeology]` into lightweight `TileSurfaceInfo` entries for `tec11-sky-view`.
pub fn tile_surface_infos(tiles: &[tile::TileGeology]) -> Vec<tec11_sky_view::TileSurfaceInfo> {
    tiles
        .iter()
        .map(|t| {
            let (rock_name, rock_kind_code) = if let Some(top) = t.layers.last() {
                (top.rock.name, top.rock.kind.code())
            } else {
                ("basalt", "I")
            };
            tec11_sky_view::TileSurfaceInfo {
                elev_km: t.elevation(),
                mountainosity_norm: t.mountainosity_norm(),
                top_felsity: t.top_felsity(),
                rock_name,
                rock_kind_code,
            }
        })
        .collect()
}

pub use pangea::PangeaWorld;
pub use rivers::{trace_coarse_rivers, RiversSandbox};
pub use rock::{Rock, RockKind, RockLayer};
pub use tectonics::{
    classify_submarine_tile, MantleHotspot, OceanicSubductionArc, SpreadingRidgeArc,
    TectonicsSimulation,
};
pub use tile::{planet_radius_km, tiles_from_snapshot, LithosphericGroup, TileGeology};
