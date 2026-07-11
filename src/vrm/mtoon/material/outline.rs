use crate::prelude::*;
use bevy::asset::Asset;
use bevy::color::LinearRgba;
use bevy::prelude::*;

#[derive(Asset, PartialEq, Debug, Clone, Default, Reflect)]
#[reflect(Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", reflect(Serialize, Deserialize))]
pub struct MToonOutline {
    /// [`OutlineWidthMode`]
    pub mode: OutlineWidthMode,
    /// The outline width
    ///
    /// the unit is in meters.  
    /// The outline width.
    pub width_factor: f32,

    /// The outline color.
    pub color: LinearRgba,

    /// The factor for the outline lighting mix.
    pub lighting_mix_factor: f32,
}

impl From<&VrmcMaterialsExtensitions> for MToonOutline {
    fn from(value: &VrmcMaterialsExtensitions) -> Self {
        let color = value.outline_color_factor;
        Self {
            mode: match value.outline_width_mode.as_str() {
                "worldCoordinates" => OutlineWidthMode::WorldCoordinates,
                "screenCoordinates" => OutlineWidthMode::ScreenCoordinates,
                _ => OutlineWidthMode::None,
            },
            width_factor: value.outline_width_factor.unwrap_or_default(),
            lighting_mix_factor: value.outline_lighting_mix_factor,
            color: LinearRgba::rgb(color[0], color[1], color[2]),
        }
    }
}

/// Please see [`outlinewidthmode`](https://github.com/vrm-c/vrm-specification/blob/master/specification/VRMC_materials_mtoon-1.0/README.md#outlinewidthmode) for details.
#[derive(Reflect, Debug, Clone, Default, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", reflect(Serialize, Deserialize))]
pub enum OutlineWidthMode {
    /// The outline will not be drawn.
    #[default]
    None,
    /// The outline width is determined by the distance in world coordinates.
    WorldCoordinates,
    /// The outline width is determined by the screen coordinates (ratio of screen height).
    ///
    /// Note: this crate's built-in `MToon` renderer does not implement this mode yet;
    /// it currently renders no outline (same as [`OutlineWidthMode::None`]).
    /// The parsed value is preserved so that external renderers can consume it.
    ScreenCoordinates,
}
