//! A client surface drawn through the rounded-corner clip.

use smithay::backend::renderer::{
    Frame,
    element::{
        Element, Id, Kind, RenderElement, UnderlyingStorage, surface::WaylandSurfaceRenderElement,
    },
    gles::{GlesError, GlesFrame, GlesRenderer, GlesTexProgram, Uniform},
    utils::{CommitCounter, DamageSet, OpaqueRegions},
};
use smithay::utils::{
    Buffer as BufferCoords, Physical, Rectangle, Scale, Transform, user_data::UserDataMap,
};

use super::clip::frag_to_local;

/// Wraps a surface element so that nothing shows outside `clip`, whose
/// corners are rounded by `radii` (top-left, top-right, bottom-right,
/// bottom-left).
#[derive(Debug)]
pub struct ClippedSurface {
    inner: WaylandSurfaceRenderElement<GlesRenderer>,
    program: GlesTexProgram,
    clip: Rectangle<i32, Physical>,
    radii: [f32; 4],
}

impl ClippedSurface {
    pub fn new(
        inner: WaylandSurfaceRenderElement<GlesRenderer>,
        program: GlesTexProgram,
        clip: Rectangle<i32, Physical>,
        radii: [f32; 4],
    ) -> Self {
        Self {
            inner,
            program,
            clip,
            radii,
        }
    }
}

impl Element for ClippedSurface {
    fn id(&self) -> &Id {
        self.inner.id()
    }

    fn current_commit(&self) -> CommitCounter {
        self.inner.current_commit()
    }

    fn src(&self) -> Rectangle<f64, BufferCoords> {
        self.inner.src()
    }

    fn transform(&self) -> Transform {
        self.inner.transform()
    }

    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.inner.geometry(scale)
    }

    fn damage_since(
        &self,
        scale: Scale<f64>,
        commit: Option<CommitCounter>,
    ) -> DamageSet<i32, Physical> {
        self.inner.damage_since(scale, commit)
    }

    /// Nothing is declared opaque: the clip makes corners see-through, and
    /// what lies under them must still be drawn.
    fn opaque_regions(&self, _scale: Scale<f64>) -> OpaqueRegions<i32, Physical> {
        OpaqueRegions::default()
    }

    fn alpha(&self) -> f32 {
        self.inner.alpha()
    }

    fn kind(&self) -> Kind {
        self.inner.kind()
    }
}

impl RenderElement<GlesRenderer> for ClippedSurface {
    fn draw(
        &self,
        frame: &mut GlesFrame<'_, '_>,
        src: Rectangle<f64, BufferCoords>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        _opaque_regions: &[Rectangle<i32, Physical>],
        cache: Option<&UserDataMap>,
    ) -> Result<(), GlesError> {
        let viewport = frame.output_size();
        let (row_x, row_y) = frag_to_local(
            frame.projection(),
            [viewport.w as f32, viewport.h as f32],
            [self.clip.loc.x as f32, self.clip.loc.y as f32],
        );
        let size = [self.clip.size.w as f32, self.clip.size.h as f32];

        frame.override_default_tex_program(
            self.program.clone(),
            vec![
                Uniform::new("clip_row_x", row_x),
                Uniform::new("clip_row_y", row_y),
                Uniform::new("clip_size", size),
                Uniform::new("clip_radius", self.radii),
            ],
        );
        // Blending must stay on everywhere, so no opaque region is passed.
        let result = self.inner.draw(frame, src, dst, damage, &[], cache);
        frame.clear_tex_program_override();
        result
    }

    /// Never scanned out directly: the clip only exists in the shader.
    fn underlying_storage(&self, _renderer: &mut GlesRenderer) -> Option<UnderlyingStorage<'_>> {
        None
    }
}
