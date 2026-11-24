//! Material palette (`MaterialId`), compact 4-byte `HexPrismCell` voxel records,
//! RGB visualization colors, and mappings from Tec11 `Rock` & mineral occurrences.

use tec11_geology::rock::{Rock, RockKind};

/// Compact material identifier covering Air, surficial mantles/fluids, all Tec11 rock types,
/// and discrete ore/mineral vein voxels (`data-model.md`: `MaterialId`).
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
#[repr(transparent)]
pub struct MaterialId(pub u16);

impl MaterialId {
    // 0: Air
    pub const AIR: Self = Self(0);

    // 1..=15: Fluids & surficial mantles
    pub const WATER: Self = Self(1);
    pub const OCEAN_WATER: Self = Self(2);
    pub const ICE: Self = Self(3);
    pub const SNOW: Self = Self(4);
    pub const TOPSOIL: Self = Self(5);
    pub const LOESS_TILL: Self = Self(6);
    pub const ALLUVIAL_SILT: Self = Self(7);
    pub const RIVER_GRAVEL: Self = Self(8);
    pub const SAND: Self = Self(9);
    pub const PEAT: Self = Self(10);
    pub const TALUS: Self = Self(11);
    pub const SALT_CRUST: Self = Self(12);

    // 16..=31: Igneous rocks
    pub const BASALT: Self = Self(16);
    pub const ANDESITE: Self = Self(17);
    pub const RHYOLITE: Self = Self(18);
    pub const GABBRO: Self = Self(19);
    pub const DIORITE: Self = Self(20);
    pub const GRANITE: Self = Self(21);
    pub const KIMBERLITE: Self = Self(22);

    // 32..=47: Sedimentary rocks
    pub const CLAYSTONE: Self = Self(32);
    pub const SILTSTONE: Self = Self(33);
    pub const SANDSTONE: Self = Self(34);
    pub const CONGLOMERATE: Self = Self(35);
    pub const LIMESTONE: Self = Self(36);
    pub const CHALK: Self = Self(37);
    pub const IMPACT_BRECCIA: Self = Self(38);

    // 48..=63: Metamorphic rocks
    pub const SLATE: Self = Self(48);
    pub const PHYLLITE: Self = Self(49);
    pub const SCHIST: Self = Self(50);
    pub const GREENSCHIST: Self = Self(51);
    pub const AMPHIBOLITE: Self = Self(52);
    pub const ECLOGITE: Self = Self(53);
    pub const GNEISS: Self = Self(54);
    pub const GRANULITE: Self = Self(55);
    pub const MARBLE: Self = Self(56);
    pub const QUARTZITE: Self = Self(57);
    pub const HORNFELS: Self = Self(58);
    pub const BLUESCHIST: Self = Self(59);

    // 64..=95: Discrete ore & mineral vein voxels
    pub const NATIVE_COPPER: Self = Self(64);
    pub const IRON_ORE: Self = Self(65);
    pub const GOLD_VEIN: Self = Self(66);
    pub const SILVER_VEIN: Self = Self(67);
    pub const TIN_ORE: Self = Self(68);
    pub const COAL_SEAM: Self = Self(69);
    pub const QUARTZ_VEIN: Self = Self(70);
    pub const GEMSTONE_VEIN: Self = Self(71);
    pub const SULFUR_DEPOSIT: Self = Self(72);
    pub const KIMBERLITE_DIAMOND: Self = Self(73);

    #[inline]
    pub fn is_air(self) -> bool {
        self == Self::AIR
    }

    #[inline]
    pub fn is_fluid(self) -> bool {
        matches!(self, Self::WATER | Self::OCEAN_WATER)
    }

    #[inline]
    pub fn is_transparent(self) -> bool {
        self.is_air() || self.is_fluid()
    }

    #[inline]
    pub fn is_ore(self) -> bool {
        self.0 >= 64
    }

    #[inline]
    pub fn is_surficial(self) -> bool {
        self.0 >= 3 && self.0 <= 15
    }

    /// Map a Tec11 `Rock` to its exact canonical `MaterialId`.
    pub fn from_rock(rock: &Rock) -> Self {
        match rock.kind {
            RockKind::Igneous => {
                if rock.felsity <= 0.35 && rock.bogginess.unwrap_or(0.0) > 0.05 {
                    return Self::KIMBERLITE;
                }
                match rock.name {
                    "basalt" => Self::BASALT,
                    "andesite" => Self::ANDESITE,
                    "rhyolite" => Self::RHYOLITE,
                    "gabbro" => Self::GABBRO,
                    "diorite" => Self::DIORITE,
                    "granite" => Self::GRANITE,
                    _ => {
                        if rock.felsity < 0.48 {
                            Self::BASALT
                        } else if rock.felsity < 0.65 {
                            Self::ANDESITE
                        } else {
                            Self::GRANITE
                        }
                    }
                }
            }
            RockKind::Sedimentary => match rock.name {
                "claystone" => Self::CLAYSTONE,
                "siltstone" => Self::SILTSTONE,
                "sandstone" => Self::SANDSTONE,
                "conglomerate" => {
                    if (rock.clasticity.unwrap_or(1.0) - 0.4213).abs() < 0.01 {
                        Self::IMPACT_BRECCIA
                    } else {
                        Self::CONGLOMERATE
                    }
                }
                "limestone" => Self::LIMESTONE,
                "chalk" => Self::CHALK,
                _ => Self::SANDSTONE,
            },
            RockKind::Metamorphic => match rock.name {
                "slate" => Self::SLATE,
                "phyllite" => Self::PHYLLITE,
                "schist" => Self::SCHIST,
                "greenschist" => Self::GREENSCHIST,
                "amphibolite" => Self::AMPHIBOLITE,
                "eclogite" => Self::ECLOGITE,
                "gneiss" => Self::GNEISS,
                "granulite" => Self::GRANULITE,
                "marble" => Self::MARBLE,
                "quartzite" => Self::QUARTZITE,
                "hornfels" => Self::HORNFELS,
                "blueschist" => Self::BLUESCHIST,
                _ => Self::GNEISS,
            },
        }
    }

    /// Map a Tec11 mineral code (from `DepositOccurrence::minerals`) to a discrete vein `MaterialId`.
    pub fn from_mineral_code(code: &str, label: &str) -> Self {
        let lower_label = label.to_ascii_lowercase();
        if lower_label.contains("diamond") || lower_label.contains("kimberlite") {
            return Self::KIMBERLITE_DIAMOND;
        }
        if lower_label.contains("coal") || lower_label.contains("lignite") || lower_label.contains("anthracite") {
            return Self::COAL_SEAM;
        }
        if lower_label.contains("gold") || lower_label.contains("auriferous") {
            return Self::GOLD_VEIN;
        }
        if lower_label.contains("silver") {
            return Self::SILVER_VEIN;
        }
        if lower_label.contains("ruby") || lower_label.contains("emerald") || lower_label.contains("jade") || lower_label.contains("gem") {
            return Self::GEMSTONE_VEIN;
        }
        match code {
            "Cu" | "CuS" => Self::NATIVE_COPPER,
            "Fe" | "BIF" => Self::IRON_ORE,
            "Au" => Self::GOLD_VEIN,
            "Ag" | "Pb" | "Zn" => Self::SILVER_VEIN,
            "Sn" | "W" => Self::TIN_ORE,
            "C" | "Coal" => Self::COAL_SEAM,
            "S" => Self::SULFUR_DEPOSIT,
            "Gem" | "Jade" => Self::GEMSTONE_VEIN,
            "Diamond" => Self::KIMBERLITE_DIAMOND,
            _ => Self::QUARTZ_VEIN,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::AIR => "Air",
            Self::WATER => "Fresh Water",
            Self::OCEAN_WATER => "Ocean Water",
            Self::ICE => "Glacial Ice",
            Self::SNOW => "Snowpack",
            Self::TOPSOIL => "Topsoil",
            Self::LOESS_TILL => "Periglacial Loess / Till",
            Self::ALLUVIAL_SILT => "Alluvial Silt",
            Self::RIVER_GRAVEL => "River Gravel",
            Self::SAND => "Windblown Sand",
            Self::PEAT => "Peat Bog",
            Self::TALUS => "Talus / Scree",
            Self::SALT_CRUST => "Evaporite Salt Crust",
            Self::BASALT => "Basalt",
            Self::ANDESITE => "Andesite",
            Self::RHYOLITE => "Rhyolite",
            Self::GABBRO => "Gabbro",
            Self::DIORITE => "Diorite",
            Self::GRANITE => "Granite",
            Self::KIMBERLITE => "Kimberlite",
            Self::CLAYSTONE => "Claystone",
            Self::SILTSTONE => "Siltstone",
            Self::SANDSTONE => "Sandstone",
            Self::CONGLOMERATE => "Conglomerate",
            Self::LIMESTONE => "Limestone",
            Self::CHALK => "Chalk",
            Self::IMPACT_BRECCIA => "Impact Breccia",
            Self::SLATE => "Slate",
            Self::PHYLLITE => "Phyllite",
            Self::SCHIST => "Schist",
            Self::GREENSCHIST => "Greenschist",
            Self::AMPHIBOLITE => "Amphibolite",
            Self::ECLOGITE => "Eclogite",
            Self::GNEISS => "Gneiss",
            Self::GRANULITE => "Granulite",
            Self::MARBLE => "Marble",
            Self::QUARTZITE => "Quartzite",
            Self::HORNFELS => "Hornfels",
            Self::BLUESCHIST => "Blueschist",
            Self::NATIVE_COPPER => "Copper Sulfide Vein",
            Self::IRON_ORE => "Banded Iron / Magnetite Ore",
            Self::GOLD_VEIN => "Auriferous Gold Vein",
            Self::SILVER_VEIN => "Polymetallic Ag-Pb-Zn Vein",
            Self::TIN_ORE => "Tin-Tungsten Greisen Vein",
            Self::COAL_SEAM => "Coal Seam",
            Self::QUARTZ_VEIN => "Hydrothermal Quartz Vein",
            Self::GEMSTONE_VEIN => "Gemstone / Jade Lode",
            Self::SULFUR_DEPOSIT => "Volcanic Sulfur Deposit",
            Self::KIMBERLITE_DIAMOND => "Diamondiferous Kimberlite",
            _ => "Unknown Rock",
        }
    }

    pub fn category_name(self) -> &'static str {
        match self.0 {
            0 => "Air",
            1..=2 => "Fluid",
            3..=15 => "Surficial",
            16..=31 => "Igneous",
            32..=47 => "Sedimentary",
            48..=63 => "Metamorphic",
            64..=95 => "Ore / Vein",
            _ => "Unknown",
        }
    }

    /// Linear-ish sRGB color `[r, g, b]` in `0.0..=1.0` for Three.js vertex colors.
    pub fn rgb(self) -> [f32; 3] {
        match self {
            Self::AIR => [0.0, 0.0, 0.0],
            Self::WATER => [0.14, 0.48, 0.78],
            Self::OCEAN_WATER => [0.08, 0.28, 0.58],
            Self::ICE => [0.76, 0.90, 0.98],
            Self::SNOW => [0.93, 0.96, 0.99],
            Self::TOPSOIL => [0.32, 0.44, 0.22],
            Self::LOESS_TILL => [0.58, 0.54, 0.34],
            Self::ALLUVIAL_SILT => [0.46, 0.42, 0.30],
            Self::RIVER_GRAVEL => [0.52, 0.50, 0.47],
            Self::SAND => [0.84, 0.74, 0.50],
            Self::PEAT => [0.24, 0.20, 0.14],
            Self::TALUS => [0.48, 0.47, 0.45],
            Self::SALT_CRUST => [0.90, 0.89, 0.85],
            Self::BASALT => [0.22, 0.23, 0.26],
            Self::ANDESITE => [0.38, 0.39, 0.42],
            Self::RHYOLITE => [0.68, 0.56, 0.54],
            Self::GABBRO => [0.19, 0.22, 0.24],
            Self::DIORITE => [0.46, 0.47, 0.49],
            Self::GRANITE => [0.74, 0.64, 0.60],
            Self::KIMBERLITE => [0.24, 0.30, 0.28],
            Self::CLAYSTONE => [0.50, 0.42, 0.36],
            Self::SILTSTONE => [0.60, 0.50, 0.39],
            Self::SANDSTONE => [0.76, 0.62, 0.42],
            Self::CONGLOMERATE => [0.64, 0.52, 0.40],
            Self::LIMESTONE => [0.78, 0.76, 0.68],
            Self::CHALK => [0.89, 0.88, 0.82],
            Self::IMPACT_BRECCIA => [0.52, 0.40, 0.36],
            Self::SLATE => [0.30, 0.34, 0.40],
            Self::PHYLLITE => [0.40, 0.44, 0.46],
            Self::SCHIST => [0.46, 0.48, 0.44],
            Self::GREENSCHIST => [0.28, 0.45, 0.34],
            Self::AMPHIBOLITE => [0.22, 0.28, 0.28],
            Self::ECLOGITE => [0.34, 0.42, 0.28],
            Self::GNEISS => [0.56, 0.52, 0.50],
            Self::GRANULITE => [0.62, 0.55, 0.48],
            Self::MARBLE => [0.88, 0.86, 0.84],
            Self::QUARTZITE => [0.80, 0.78, 0.74],
            Self::HORNFELS => [0.34, 0.31, 0.36],
            Self::BLUESCHIST => [0.28, 0.38, 0.58],
            Self::NATIVE_COPPER => [0.82, 0.46, 0.24],
            Self::IRON_ORE => [0.62, 0.22, 0.16],
            Self::GOLD_VEIN => [0.96, 0.80, 0.22],
            Self::SILVER_VEIN => [0.80, 0.84, 0.90],
            Self::TIN_ORE => [0.58, 0.60, 0.64],
            Self::COAL_SEAM => [0.10, 0.10, 0.12],
            Self::QUARTZ_VEIN => [0.94, 0.94, 0.96],
            Self::GEMSTONE_VEIN => [0.18, 0.78, 0.58],
            Self::SULFUR_DEPOSIT => [0.92, 0.88, 0.22],
            Self::KIMBERLITE_DIAMOND => [0.46, 0.86, 0.96],
            _ => [0.5, 0.5, 0.5],
        }
    }
}

/// Bitflags for `HexPrismCell.flags` (`data-model.md`).
pub mod cell_flags {
    pub const EXPOSED_SURFACE: u8 = 1 << 0;
    pub const EXCAVATED_BY_DELTA: u8 = 1 << 1;
    pub const FLORA_GRASS: u8 = 1 << 2;
    pub const FLORA_SHRUB: u8 = 1 << 3;
    pub const FLORA_TREE: u8 = 1 << 4;
    pub const NATURAL_CAVE: u8 = 1 << 5;
}

/// Compact 4-byte hex-prism voxel record (`data-model.md`: `HexPrismCell`).
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
#[repr(C)]
pub struct HexPrismCell {
    pub material: MaterialId,
    pub fluid: u8,
    pub flags: u8,
}

impl HexPrismCell {
    pub const AIR: Self = Self {
        material: MaterialId::AIR,
        fluid: 0,
        flags: 0,
    };

    #[inline]
    pub const fn solid(material: MaterialId) -> Self {
        Self {
            material,
            fluid: 0,
            flags: 0,
        }
    }

    #[inline]
    pub const fn water(is_ocean: bool, depth_level: u8) -> Self {
        Self {
            material: if is_ocean {
                MaterialId::OCEAN_WATER
            } else {
                MaterialId::WATER
            },
            fluid: if depth_level == 0 { 1 } else { depth_level },
            flags: 0,
        }
    }

    #[inline]
    pub fn is_air(self) -> bool {
        self.material.is_air()
    }

    #[inline]
    pub fn is_solid(self) -> bool {
        !self.material.is_transparent()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_prism_cell_size_is_4_bytes() {
        assert_eq!(std::mem::size_of::<HexPrismCell>(), 4);
    }
}
