//! Surface colours: one default table, optional caller overrides.

use std::collections::HashMap;

/// Walls, ground, mesh blocks, and any unmapped surface.
/// Same as BIMFabrikHH `_DEFAULT_RGB` / `_DEFAULT_CITY_RGB`.
pub const DEFAULT_RGB: [f64; 3] = [1.0, 1.0, 0.498];
/// Roofs. Same as BIMFabrikHH `_ROOF_COLOR`.
pub const ROOF_RGB: [f64; 3] = [1.0, 0.0, 0.0];
/// Nullpunktobjekt pyramid. Same as BIMFabrikHH `PYRAMID_COLOR`.
pub const BASEPOINT_RGB: [f64; 3] = [239.0 / 255.0, 109.0 / 255.0, 109.0 / 255.0];
/// City / intern Nullpunkt edge length in metres (`_DEFAULT_BASEPOINT_SIZE`).
pub const BASEPOINT_SIZE: f64 = 8.0;
/// Tree trunk. Same as TreesGenericApp `DEFAULT_TRUNK_COLOR` (112, 69, 46).
pub const TRUNK_RGB: [f64; 3] = [112.0 / 255.0, 69.0 / 255.0, 46.0 / 255.0];
/// Tree crown. Same as TreesGenericApp `DEFAULT_CROWN_COLOR` (33, 128, 46).
pub const CROWN_RGB: [f64; 3] = [33.0 / 255.0, 128.0 / 255.0, 46.0 / 255.0];
/// DGM. Same as TerrainGenericApp `_DEFAULT_TERRAIN_RGB` (102, 204, 0).
pub const TERRAIN_RGB: [f64; 3] = [102.0 / 255.0, 204.0 / 255.0, 0.0];

/// Caller colour input. Missing keys (and [`Colors::Defaults`]) use [`default_rgb`].
#[derive(Clone, Debug, Default)]
pub enum Colors {
    #[default]
    Defaults,
    /// One RGB for every product.
    Uniform([f64; 3]),
    /// Per surface type (and `"Building"` in mesh mode). Unmapped keys fall back
    /// to [`default_rgb`].
    Map(HashMap<String, [f64; 3]>),
}

pub fn default_rgb(key: &str) -> [f64; 3] {
    match key {
        "RoofSurface" => ROOF_RGB,
        _ => DEFAULT_RGB,
    }
}

impl Colors {
    pub fn rgb(&self, key: &str) -> [f64; 3] {
        match self {
            Colors::Defaults => default_rgb(key),
            Colors::Uniform(rgb) => *rgb,
            Colors::Map(map) => map.get(key).copied().unwrap_or_else(|| default_rgb(key)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_split_roof_from_the_rest() {
        assert_eq!(default_rgb("RoofSurface"), ROOF_RGB);
        assert_eq!(default_rgb("WallSurface"), DEFAULT_RGB);
        assert_eq!(default_rgb("Building"), DEFAULT_RGB);
    }

    #[test]
    fn map_overrides_one_key_and_keeps_defaults() {
        let colors = Colors::Map(HashMap::from([(
            "WallSurface".into(),
            [0.2, 0.2, 0.2],
        )]));
        assert_eq!(colors.rgb("WallSurface"), [0.2, 0.2, 0.2]);
        assert_eq!(colors.rgb("RoofSurface"), ROOF_RGB);
    }

    #[test]
    fn uniform_applies_to_every_key() {
        let colors = Colors::Uniform([0.0, 1.0, 0.0]);
        assert_eq!(colors.rgb("RoofSurface"), [0.0, 1.0, 0.0]);
        assert_eq!(colors.rgb("Building"), [0.0, 1.0, 0.0]);
    }
}
