//! Custom GLES programs.

use smithay::backend::renderer::gles::{
    GlesError, GlesPixelProgram, GlesRenderer, GlesTexProgram, UniformName, UniformType,
};

pub struct Shaders {
    /// Rounded rectangle, filled or ring, in one color or in a ramp from
    /// top to bottom: accent ring, window background.
    pub rounded: GlesPixelProgram,
    /// Draws a surface clipped to a rounded rectangle.
    pub clip: GlesTexProgram,
}

impl Shaders {
    pub fn compile(renderer: &mut GlesRenderer) -> Result<Self, GlesError> {
        let rounded = renderer.compile_custom_pixel_shader(
            include_str!("shaders/rounded.frag"),
            &[
                UniformName::new("shape_size", UniformType::_2f),
                UniformName::new("shape_offset", UniformType::_2f),
                UniformName::new("color", UniformType::_4f),
                UniformName::new("ramp", UniformType::_1f),
                UniformName::new("stop_0", UniformType::_4f),
                UniformName::new("stop_1", UniformType::_4f),
                UniformName::new("stop_2", UniformType::_4f),
                UniformName::new("stop_3", UniformType::_4f),
                UniformName::new("stop_4", UniformType::_4f),
                UniformName::new("stop_5", UniformType::_4f),
                UniformName::new("stop_6", UniformType::_4f),
                UniformName::new("stop_7", UniformType::_4f),
                UniformName::new("stops_upper", UniformType::_4f),
                UniformName::new("stops_lower", UniformType::_4f),
                UniformName::new("radius", UniformType::_4f),
                UniformName::new("border", UniformType::_1f),
            ],
        )?;
        let clip = renderer.compile_custom_texture_shader(
            include_str!("shaders/clip.frag"),
            &[
                UniformName::new("clip_row_x", UniformType::_3f),
                UniformName::new("clip_row_y", UniformType::_3f),
                UniformName::new("clip_size", UniformType::_2f),
                UniformName::new("clip_radius", UniformType::_4f),
            ],
        )?;
        Ok(Self { rounded, clip })
    }
}
