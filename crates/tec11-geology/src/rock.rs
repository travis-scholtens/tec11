#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RockKind {
    Igneous,
    Sedimentary,
    Metamorphic,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Rock {
    pub kind: RockKind,
    pub name: &'static str,
    pub felsity: f32,
    pub toughness: f32,
}

impl Rock {
    /// Matches `rock/igneous.py:extrusive(silica)`.
    pub fn extrusive(silica: f32) -> Self {
        let name = if silica > 0.63 {
            "rhyolite"
        } else if silica > 0.52 {
            "dacite"
        } else {
            "basalt"
        };
        Self {
            kind: RockKind::Igneous,
            name,
            felsity: silica,
            toughness: silica,
        }
    }

    /// Matches `rock/igneous.py:intrusive(silica)`.
    pub fn intrusive(silica: f32) -> Self {
        let name = if silica > 0.63 {
            "granite"
        } else if silica > 0.52 {
            "andesite"
        } else {
            "gabbro"
        };
        Self {
            kind: RockKind::Igneous,
            name,
            felsity: silica,
            toughness: silica,
        }
    }

    /// Standard basaltic seafloor (`PlanetSimulation.seafloor()`).
    pub fn seafloor() -> Self {
        Self::extrusive(0.5)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RockLayer {
    pub rock: Rock,
    pub thickness: f32,
}
