//! Soft Gaussian shadows behind floating windows and the placement bar.

use coquille_model::{Point, Rect};
use smithay::backend::renderer::{
    element::Kind,
    gles::{GlesPixelProgram, Uniform, element::PixelShaderElement},
};

use crate::theme::Rgba;

/// Specification of a soft rounded rectangular shadow.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shadow {
    /// The shape that casts the shadow.
    pub shape: Rect,
    /// Offset of the shadow relative to the shape.
    pub offset: Point,
    /// Gaussian blur standard deviation in pixels.
    pub blur: f32,
    /// Premultiplied shadow color.
    pub color: Rgba,
    /// Corner radii of the shape: top-left, top-right, bottom-right, bottom-left.
    pub radii: [f32; 4],
}

impl Shadow {
    /// The bounding area covering the blurred shadow.
    pub fn area(&self) -> Rect {
        let margin = (self.blur * 3.0).ceil() as i32;
        Rect::new(
            self.shape.x + self.offset.x - margin,
            self.shape.y + self.offset.y - margin,
            self.shape.w + 2 * margin,
            self.shape.h + 2 * margin,
        )
    }
}

/// Keeps a shadow's element between frames so an unchanged shadow causes no damage.
#[derive(Default)]
pub struct ShadowCache(Option<(Shadow, PixelShaderElement)>);

impl ShadowCache {
    pub fn element(&mut self, program: &GlesPixelProgram, shadow: Shadow) -> PixelShaderElement {
        if let Some((cached, element)) = &self.0 {
            if *cached == shadow {
                return element.clone();
            }
        }
        let element = shadow_element(program, shadow);
        self.0 = Some((shadow, element.clone()));
        element
    }
}

pub fn shadow_element(program: &GlesPixelProgram, shadow: Shadow) -> PixelShaderElement {
    let area = shadow.area();
    let shape_offset = [
        (area.x - (shadow.shape.x + shadow.offset.x)) as f32,
        (area.y - (shadow.shape.y + shadow.offset.y)) as f32,
    ];
    PixelShaderElement::new(
        program.clone(),
        crate::render::scene::rectangle(area),
        None,
        1.0,
        vec![
            Uniform::new("shape_size", [shadow.shape.w as f32, shadow.shape.h as f32]),
            Uniform::new("shape_offset", shape_offset),
            Uniform::new("color", shadow.color.premultiplied()),
            Uniform::new("radius", shadow.radii),
            Uniform::new("blur", shadow.blur),
        ],
        Kind::Unspecified,
    )
}
