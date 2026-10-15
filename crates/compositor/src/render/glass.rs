//! Glass element with Dual Kawase background blur.

// Items will be consumed in the compositor scene by tickets 04 and 05.
#![allow(dead_code)]

use std::sync::Mutex;

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::{
    Frame, FrameContext, Offscreen,
    element::{Element, Id, Kind, RenderElement, UnderlyingStorage},
    gles::{GlesError, GlesFrame, GlesRenderer, GlesTexture, Uniform, ffi},
    utils::{CommitCounter, DamageSet, OpaqueRegions},
};
use smithay::utils::{
    Buffer as BufferCoord, Physical, Rectangle, Scale, Size, Transform, user_data::UserDataMap,
};

use super::clip::frag_to_local;
use super::shaders::Shaders;

/// Shape mask defining which part of the blurred background is visible.
#[derive(Debug, Clone)]
pub enum Mask {
    /// Rounded rectangle with specified corner radii (top-left, top-right, bottom-right, bottom-left).
    Rounded { radii: [f32; 4] },
    /// Alpha channel mask from a surface texture, with optional corner radii.
    Alpha {
        texture: GlesTexture,
        src: Rectangle<f64, BufferCoord>,
        dst: Rectangle<i32, Physical>,
        radii: [f32; 4],
    },
}

impl PartialEq for Mask {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Mask::Rounded { radii: r1 }, Mask::Rounded { radii: r2 }) => r1 == r2,
            (
                Mask::Alpha {
                    texture: t1,
                    src: s1,
                    dst: d1,
                    radii: r1,
                },
                Mask::Alpha {
                    texture: t2,
                    src: s2,
                    dst: d2,
                    radii: r2,
                },
            ) => t1.tex_id() == t2.tex_id() && s1 == s2 && d1 == d2 && r1 == r2,
            _ => false,
        }
    }
}

/// Clamp blur strength to 1..=5.
pub fn clamp_strength(strength: u8) -> u8 {
    strength.clamp(1, 5)
}

/// Margin added around the shape to capture enough context for blurring.
pub fn blur_margin(strength: u8) -> i32 {
    let s = clamp_strength(strength) as i32;
    8 * s
}

/// Expand the shape by the blur margin and clip it to output bounds.
pub fn expanded_and_clipped_geometry(
    shape: Rectangle<i32, Physical>,
    margin: i32,
    output_size: Size<i32, Physical>,
) -> Rectangle<i32, Physical> {
    let output_rect = Rectangle::from_size(output_size);
    let expanded = Rectangle::new(
        (shape.loc.x - margin, shape.loc.y - margin).into(),
        (shape.size.w + 2 * margin, shape.size.h + 2 * margin).into(),
    );
    expanded.intersection(output_rect).unwrap_or_default()
}

/// A render element that captures and blurs what is behind it.
#[derive(Debug, Clone)]
pub struct GlassElement {
    id: Id,
    commit: CommitCounter,
    shaders: Shaders,
    shape: Rectangle<i32, Physical>,
    geometry: Rectangle<i32, Physical>,
    output_size: Size<i32, Physical>,
    mask: Mask,
    strength: u8,
}

impl GlassElement {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Id,
        commit: CommitCounter,
        shaders: Shaders,
        shape: Rectangle<i32, Physical>,
        output_size: Size<i32, Physical>,
        mask: Mask,
        strength: u8,
    ) -> Self {
        let margin = blur_margin(strength);
        let geometry = expanded_and_clipped_geometry(shape, margin, output_size);
        Self {
            id,
            commit,
            shaders,
            shape,
            geometry,
            output_size,
            mask,
            strength,
        }
    }
}

/// Persistent cache keeping an element's Id and commit counter between frames.
#[derive(Debug)]
pub struct GlassCache {
    id: Id,
    commit: CommitCounter,
    last_spec: Option<(Rectangle<i32, Physical>, Mask, u8)>,
}

impl Default for GlassCache {
    fn default() -> Self {
        Self {
            id: Id::new(),
            commit: CommitCounter::default(),
            last_spec: None,
        }
    }
}

impl GlassCache {
    pub fn element(
        &mut self,
        shaders: &Shaders,
        shape: Rectangle<i32, Physical>,
        output_size: Size<i32, Physical>,
        mask: Mask,
        strength: u8,
    ) -> GlassElement {
        let spec = (shape, mask.clone(), strength);
        if self.last_spec.as_ref() != Some(&spec) {
            self.commit.increment();
            self.last_spec = Some(spec);
        }
        GlassElement::new(
            self.id.clone(),
            self.commit,
            shaders.clone(),
            shape,
            output_size,
            mask,
            strength,
        )
    }
}

impl Element for GlassElement {
    fn id(&self) -> &Id {
        &self.id
    }

    fn current_commit(&self) -> CommitCounter {
        self.commit
    }

    fn src(&self) -> Rectangle<f64, BufferCoord> {
        Rectangle::from_size((self.geometry.size.w as f64, self.geometry.size.h as f64).into())
    }

    fn geometry(&self, _scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.geometry
    }

    fn transform(&self) -> Transform {
        Transform::Normal
    }

    fn damage_since(
        &self,
        _scale: Scale<f64>,
        commit: Option<CommitCounter>,
    ) -> DamageSet<i32, Physical> {
        if commit == Some(self.commit) {
            DamageSet::default()
        } else {
            DamageSet::from_slice(&[self.geometry])
        }
    }

    fn opaque_regions(&self, _scale: Scale<f64>) -> OpaqueRegions<i32, Physical> {
        OpaqueRegions::default()
    }

    fn alpha(&self) -> f32 {
        1.0
    }

    fn kind(&self) -> Kind {
        Kind::Unspecified
    }

    fn is_framebuffer_effect(&self) -> bool {
        true
    }
}

struct TextureLevel {
    tex: GlesTexture,
    fbo: u32,
    size: Size<i32, BufferCoord>,
}

#[derive(Default)]
struct BlurCache {
    down: Vec<TextureLevel>,
    up: Vec<TextureLevel>,
    source: Option<TextureLevel>,
    output: Option<GlesTexture>,
    vbo: u32,
}

impl RenderElement<GlesRenderer> for GlassElement {
    // Documented unsafe rationale: interacting directly with OpenGL ES C-FFI
    // for FBO management, blitting the current framebuffer, and running Dual Kawase passes.
    #[allow(unsafe_code)]
    fn capture_framebuffer(
        &self,
        frame: &mut GlesFrame<'_, '_>,
        _src: Rectangle<f64, BufferCoord>,
        dst: Rectangle<i32, Physical>,
        cache: &UserDataMap,
    ) -> Result<(), GlesError> {
        if dst.size.w <= 0 || dst.size.h <= 0 {
            return Ok(());
        }

        let passes = clamp_strength(self.strength) as usize;
        let dst_w = dst.size.w;
        let dst_h = dst.size.h;

        let blur_cache = cache.get_or_insert_threadsafe(|| Mutex::new(BlurCache::default()));
        let mut blur_cache = blur_cache.lock().unwrap();

        let need_realloc = blur_cache.source.as_ref().map(|s| s.size)
            != Some((dst_w, dst_h).into())
            || blur_cache.down.len() != passes
            || blur_cache.up.len() != passes;

        if need_realloc {
            frame.with_context(|gl| unsafe {
                // Unsafe rationale: deleting OpenGL framebuffers during cache reallocation.
                if let Some(source) = blur_cache.source.take() {
                    gl.DeleteFramebuffers(1, &source.fbo);
                }
                for level in blur_cache.down.drain(..) {
                    gl.DeleteFramebuffers(1, &level.fbo);
                }
                for level in blur_cache.up.drain(..) {
                    gl.DeleteFramebuffers(1, &level.fbo);
                }
            })?;

            let source_tex = {
                let mut guard = FrameContext::renderer(frame);
                let renderer = guard.as_mut();
                Offscreen::<GlesTexture>::create_buffer(
                    renderer,
                    Fourcc::Abgr8888,
                    (dst_w, dst_h).into(),
                )?
            };

            let mut down_levels: Vec<(GlesTexture, Size<i32, BufferCoord>)> =
                Vec::with_capacity(passes);
            let mut cur_w = dst_w;
            let mut cur_h = dst_h;
            for _ in 0..passes {
                cur_w = (cur_w / 2).max(1);
                cur_h = (cur_h / 2).max(1);
                let tex = {
                    let mut guard = FrameContext::renderer(frame);
                    let renderer = guard.as_mut();
                    Offscreen::<GlesTexture>::create_buffer(
                        renderer,
                        Fourcc::Abgr8888,
                        (cur_w, cur_h).into(),
                    )?
                };
                down_levels.push((tex, (cur_w, cur_h).into()));
            }

            let mut up_levels: Vec<(GlesTexture, Size<i32, BufferCoord>)> =
                Vec::with_capacity(passes);
            for i in (0..passes).rev() {
                let (target_w, target_h) = if i == 0 {
                    (dst_w, dst_h)
                } else {
                    let prev = down_levels[i - 1].1;
                    (prev.w, prev.h)
                };
                let tex = {
                    let mut guard = FrameContext::renderer(frame);
                    let renderer = guard.as_mut();
                    Offscreen::<GlesTexture>::create_buffer(
                        renderer,
                        Fourcc::Abgr8888,
                        (target_w, target_h).into(),
                    )?
                };
                up_levels.push((tex, (target_w, target_h).into()));
            }

            frame.with_context(|gl| unsafe {
                // Unsafe rationale: creating framebuffers and attaching allocated offscreen textures.
                let setup_tex = |tex_id: u32| {
                    gl.BindTexture(ffi::TEXTURE_2D, tex_id);
                    gl.TexParameteri(ffi::TEXTURE_2D, ffi::TEXTURE_MIN_FILTER, ffi::LINEAR as i32);
                    gl.TexParameteri(ffi::TEXTURE_2D, ffi::TEXTURE_MAG_FILTER, ffi::LINEAR as i32);
                    gl.TexParameteri(
                        ffi::TEXTURE_2D,
                        ffi::TEXTURE_WRAP_S,
                        ffi::CLAMP_TO_EDGE as i32,
                    );
                    gl.TexParameteri(
                        ffi::TEXTURE_2D,
                        ffi::TEXTURE_WRAP_T,
                        ffi::CLAMP_TO_EDGE as i32,
                    );
                };

                setup_tex(source_tex.tex_id());

                let mut source_fbo = 0;
                gl.GenFramebuffers(1, &mut source_fbo);
                gl.BindFramebuffer(ffi::FRAMEBUFFER, source_fbo);
                gl.FramebufferTexture2D(
                    ffi::FRAMEBUFFER,
                    ffi::COLOR_ATTACHMENT0,
                    ffi::TEXTURE_2D,
                    source_tex.tex_id(),
                    0,
                );
                let status = gl.CheckFramebufferStatus(ffi::FRAMEBUFFER);
                if status != ffi::FRAMEBUFFER_COMPLETE {
                    tracing::error!(status, "source FBO incomplete");
                }
                gl.BindFramebuffer(ffi::FRAMEBUFFER, 0);

                blur_cache.source = Some(TextureLevel {
                    tex: source_tex,
                    fbo: source_fbo,
                    size: (dst_w, dst_h).into(),
                });

                for (tex, size) in down_levels {
                    setup_tex(tex.tex_id());
                    let mut fbo = 0;
                    gl.GenFramebuffers(1, &mut fbo);
                    gl.BindFramebuffer(ffi::FRAMEBUFFER, fbo);
                    gl.FramebufferTexture2D(
                        ffi::FRAMEBUFFER,
                        ffi::COLOR_ATTACHMENT0,
                        ffi::TEXTURE_2D,
                        tex.tex_id(),
                        0,
                    );
                    let status = gl.CheckFramebufferStatus(ffi::FRAMEBUFFER);
                    if status != ffi::FRAMEBUFFER_COMPLETE {
                        tracing::error!(status, "down FBO incomplete");
                    }
                    gl.BindFramebuffer(ffi::FRAMEBUFFER, 0);
                    blur_cache.down.push(TextureLevel { tex, fbo, size });
                }

                for (tex, size) in up_levels {
                    setup_tex(tex.tex_id());
                    let mut fbo = 0;
                    gl.GenFramebuffers(1, &mut fbo);
                    gl.BindFramebuffer(ffi::FRAMEBUFFER, fbo);
                    gl.FramebufferTexture2D(
                        ffi::FRAMEBUFFER,
                        ffi::COLOR_ATTACHMENT0,
                        ffi::TEXTURE_2D,
                        tex.tex_id(),
                        0,
                    );
                    let status = gl.CheckFramebufferStatus(ffi::FRAMEBUFFER);
                    if status != ffi::FRAMEBUFFER_COMPLETE {
                        tracing::error!(status, "up FBO incomplete");
                    }
                    gl.BindFramebuffer(ffi::FRAMEBUFFER, 0);
                    blur_cache.up.push(TextureLevel { tex, fbo, size });
                }

                if blur_cache.vbo == 0 {
                    let mut vbo = 0;
                    gl.GenBuffers(1, &mut vbo);
                    gl.BindBuffer(ffi::ARRAY_BUFFER, vbo);
                    static QUAD_VERTS: [f32; 8] = [-1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0];
                    gl.BufferData(
                        ffi::ARRAY_BUFFER,
                        (QUAD_VERTS.len() * std::mem::size_of::<f32>()) as isize,
                        QUAD_VERTS.as_ptr() as *const _,
                        ffi::STATIC_DRAW,
                    );
                    gl.BindBuffer(ffi::ARRAY_BUFFER, 0);
                    blur_cache.vbo = vbo;
                }
            })?;
        }

        let projection = *frame.projection();
        let out_h = self.output_size.h;
        let blur_down_prog = self.shaders.blur_down;
        let blur_up_prog = self.shaders.blur_up;
        let strength = self.strength;

        frame.with_context(|gl| unsafe {
            // Unsafe rationale: Smithay does not expose direct blit-to-texture and custom multi-pass GL filters.
            // We capture the region behind the element using glBlitFramebuffer, then run Dual Kawase
            // reduction/expansion passes.
            // All modified OpenGL states (FBO, viewport, scissor, blend, program, active texture,
            // bound texture, vertex attrib array) are explicitly restored upon completion.

            let mut prev_draw_fbo = 0;
            let mut prev_read_fbo = 0;
            gl.GetIntegerv(ffi::DRAW_FRAMEBUFFER_BINDING, &mut prev_draw_fbo);
            gl.GetIntegerv(ffi::READ_FRAMEBUFFER_BINDING, &mut prev_read_fbo);

            let mut prev_viewport = [0; 4];
            gl.GetIntegerv(ffi::VIEWPORT, prev_viewport.as_mut_ptr());

            let mut prev_program = 0;
            gl.GetIntegerv(ffi::CURRENT_PROGRAM, &mut prev_program);

            let scissor_enabled = gl.IsEnabled(ffi::SCISSOR_TEST);
            let blend_enabled = gl.IsEnabled(ffi::BLEND);

            let mut prev_active_tex = 0;
            gl.GetIntegerv(ffi::ACTIVE_TEXTURE, &mut prev_active_tex);

            let mut prev_tex0 = 0;
            gl.GetIntegerv(ffi::TEXTURE_BINDING_2D, &mut prev_tex0);

            let mut prev_vbo = 0;
            gl.GetIntegerv(ffi::ARRAY_BUFFER_BINDING, &mut prev_vbo);

            let source = blur_cache.source.as_ref().unwrap();

            // 1. Blit from current framebuffer into source texture
            // Flip vertical coordinates to match Smithay texture UV sampling orientation.
            let d = projection[4];
            let (src_y0, src_y1) = if d < 0.0 {
                // Upright GL coordinates: origin at bottom
                (out_h - dst.loc.y, out_h - (dst.loc.y + dst.size.h))
            } else {
                // Flipped GL coordinates: origin at top
                (dst.loc.y + dst.size.h, dst.loc.y)
            };
            let (src_x0, src_x1) = (dst.loc.x, dst.loc.x + dst.size.w);

            gl.BindFramebuffer(ffi::READ_FRAMEBUFFER, prev_draw_fbo as u32);
            gl.BindFramebuffer(ffi::DRAW_FRAMEBUFFER, source.fbo);
            if prev_draw_fbo == 0 {
                gl.ReadBuffer(ffi::BACK);
            } else {
                gl.ReadBuffer(ffi::COLOR_ATTACHMENT0);
            }
            gl.Disable(ffi::SCISSOR_TEST);

            gl.BlitFramebuffer(
                src_x0,
                src_y0,
                src_x1,
                src_y1,
                0,
                0,
                dst_w,
                dst_h,
                ffi::COLOR_BUFFER_BIT,
                ffi::LINEAR,
            );
            let blit_err = gl.GetError();
            if blit_err != ffi::NO_ERROR {
                tracing::error!(
                    blit_err,
                    src_x0,
                    src_y0,
                    src_x1,
                    src_y1,
                    dst_w,
                    dst_h,
                    prev_draw_fbo,
                    "BlitFramebuffer error"
                );
            }

            gl.Disable(ffi::BLEND);

            // 2. Downsampling passes
            let mut cur_tex_id = source.tex.tex_id();
            let mut cur_w = dst_w;
            let mut cur_h = dst_h;

            gl.BindBuffer(ffi::ARRAY_BUFFER, blur_cache.vbo);
            gl.UseProgram(blur_down_prog.program);
            gl.Uniform1i(blur_down_prog.u_tex, 0);
            gl.EnableVertexAttribArray(blur_down_prog.a_position);
            gl.VertexAttribPointer(
                blur_down_prog.a_position,
                2,
                ffi::FLOAT,
                ffi::FALSE,
                0,
                std::ptr::null(),
            );

            for (i, level) in blur_cache.down.iter().enumerate() {
                gl.BindFramebuffer(ffi::FRAMEBUFFER, level.fbo);
                gl.Viewport(0, 0, level.size.w, level.size.h);

                gl.ActiveTexture(ffi::TEXTURE0);
                gl.BindTexture(ffi::TEXTURE_2D, cur_tex_id);

                let halfpixel = [0.5 / cur_w as f32, 0.5 / cur_h as f32];
                let offset = 1.0 + (strength as f32) * 0.15 + (i as f32) * 0.25;
                gl.Uniform2f(blur_down_prog.u_halfpixel, halfpixel[0], halfpixel[1]);
                gl.Uniform1f(blur_down_prog.u_offset, offset);

                gl.DrawArrays(ffi::TRIANGLE_STRIP, 0, 4);

                cur_tex_id = level.tex.tex_id();
                cur_w = level.size.w;
                cur_h = level.size.h;
            }

            gl.DisableVertexAttribArray(blur_down_prog.a_position);

            // 3. Upsampling passes
            gl.UseProgram(blur_up_prog.program);
            gl.Uniform1i(blur_up_prog.u_tex, 0);
            gl.EnableVertexAttribArray(blur_up_prog.a_position);
            gl.VertexAttribPointer(
                blur_up_prog.a_position,
                2,
                ffi::FLOAT,
                ffi::FALSE,
                0,
                std::ptr::null(),
            );

            for (i, level) in blur_cache.up.iter().enumerate() {
                gl.BindFramebuffer(ffi::FRAMEBUFFER, level.fbo);
                gl.Viewport(0, 0, level.size.w, level.size.h);

                gl.ActiveTexture(ffi::TEXTURE0);
                gl.BindTexture(ffi::TEXTURE_2D, cur_tex_id);

                let halfpixel = [0.5 / cur_w as f32, 0.5 / cur_h as f32];
                let offset = 1.0 + (strength as f32) * 0.15 + ((passes - 1 - i) as f32) * 0.25;
                gl.Uniform2f(blur_up_prog.u_halfpixel, halfpixel[0], halfpixel[1]);
                gl.Uniform1f(blur_up_prog.u_offset, offset);

                gl.DrawArrays(ffi::TRIANGLE_STRIP, 0, 4);

                cur_tex_id = level.tex.tex_id();
                cur_w = level.size.w;
                cur_h = level.size.h;
            }

            gl.DisableVertexAttribArray(blur_up_prog.a_position);

            // Store final blurred texture into output
            blur_cache.output = blur_cache.up.last().map(|lvl| lvl.tex.clone());

            // Restore OpenGL state
            gl.BindBuffer(ffi::ARRAY_BUFFER, prev_vbo as u32);
            gl.BindFramebuffer(ffi::DRAW_FRAMEBUFFER, prev_draw_fbo as u32);
            gl.BindFramebuffer(ffi::READ_FRAMEBUFFER, prev_read_fbo as u32);
            gl.Viewport(
                prev_viewport[0],
                prev_viewport[1],
                prev_viewport[2],
                prev_viewport[3],
            );
            gl.UseProgram(prev_program as u32);

            if scissor_enabled == ffi::TRUE {
                gl.Enable(ffi::SCISSOR_TEST);
            } else {
                gl.Disable(ffi::SCISSOR_TEST);
            }

            if blend_enabled == ffi::TRUE {
                gl.Enable(ffi::BLEND);
            } else {
                gl.Disable(ffi::BLEND);
            }

            gl.ActiveTexture(ffi::TEXTURE0);
            gl.BindTexture(ffi::TEXTURE_2D, prev_tex0 as u32);
            gl.ActiveTexture(prev_active_tex as u32);
        })?;

        Ok(())
    }

    // Documented unsafe rationale: interacting directly with OpenGL ES C-FFI
    // to bind and unbind texture unit 1 for alpha mask sampling in glass.frag.
    #[allow(unsafe_code)]
    fn draw(
        &self,
        frame: &mut GlesFrame<'_, '_>,
        _src: Rectangle<f64, BufferCoord>,
        _dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        _opaque_regions: &[Rectangle<i32, Physical>],
        cache: Option<&UserDataMap>,
    ) -> Result<(), GlesError> {
        let Some(cache) = cache else {
            return Ok(());
        };
        let blur_cache = cache.get::<Mutex<BlurCache>>();
        let Some(blur_cache) = blur_cache else {
            return Ok(());
        };
        let blur_cache = blur_cache.lock().unwrap();
        let Some(ref output_tex) = blur_cache.output else {
            return Ok(());
        };

        let viewport = frame.output_size();
        let (row_x, row_y) = frag_to_local(
            frame.projection(),
            [viewport.w as f32, viewport.h as f32],
            [self.shape.loc.x as f32, self.shape.loc.y as f32],
        );
        let size = [self.shape.size.w as f32, self.shape.size.h as f32];

        let (mask_mode, radii, mask_tex_id) = match &self.mask {
            Mask::Rounded { radii } => (0i32, *radii, None),
            Mask::Alpha { texture, radii, .. } => (1i32, *radii, Some(texture.tex_id())),
        };

        let uniforms = vec![
            Uniform::new("clip_row_x", row_x),
            Uniform::new("clip_row_y", row_y),
            Uniform::new("shape_size", size),
            Uniform::new("radii", radii),
            Uniform::new("mask_mode", mask_mode),
            Uniform::new("mask_tex", if mask_tex_id.is_some() { 1i32 } else { 0i32 }),
            Uniform::new("saturation", 1.15f32),
        ];

        if let Some(tex_id) = mask_tex_id {
            frame.with_context(|gl| unsafe {
                // Unsafe rationale: binding the alpha mask texture to unit 1 for multi-texture sampling in glass.frag.
                gl.ActiveTexture(ffi::TEXTURE1);
                gl.BindTexture(ffi::TEXTURE_2D, tex_id);
                gl.ActiveTexture(ffi::TEXTURE0);
            })?;
        }

        let src_rect =
            Rectangle::from_size((self.geometry.size.w as f64, self.geometry.size.h as f64).into());
        let res = frame.render_texture_from_to(
            output_tex,
            src_rect,
            self.geometry,
            damage,
            &[],
            Transform::Normal,
            1.0,
            Some(&self.shaders.glass),
            &uniforms,
        );

        if mask_tex_id.is_some() {
            let _ = frame.with_context(|gl| unsafe {
                // Unsafe rationale: unbinding texture unit 1 and restoring unit 0.
                gl.ActiveTexture(ffi::TEXTURE1);
                gl.BindTexture(ffi::TEXTURE_2D, 0);
                gl.ActiveTexture(ffi::TEXTURE0);
            });
        }

        res
    }

    fn underlying_storage(&self, _renderer: &mut GlesRenderer) -> Option<UnderlyingStorage<'_>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strength_is_clamped_to_one_to_five() {
        assert_eq!(clamp_strength(0), 1);
        assert_eq!(clamp_strength(1), 1);
        assert_eq!(clamp_strength(3), 3);
        assert_eq!(clamp_strength(5), 5);
        assert_eq!(clamp_strength(6), 5);
        assert_eq!(clamp_strength(100), 5);
    }

    #[test]
    fn blur_margin_scales_with_strength() {
        assert_eq!(blur_margin(1), 8);
        assert_eq!(blur_margin(2), 16);
        assert_eq!(blur_margin(3), 24);
        assert_eq!(blur_margin(4), 32);
        assert_eq!(blur_margin(5), 40);
        // Clamping ensures bounds
        assert_eq!(blur_margin(0), 8);
        assert_eq!(blur_margin(10), 40);
    }

    #[test]
    fn geometry_expansion_and_clipping_to_output() {
        let output: Size<i32, Physical> = (1920, 1080).into();

        // 1. Fully interior shape expands by margin on all 4 sides
        let shape = Rectangle::new((100, 100).into(), (200, 150).into());
        let margin = 24;
        let geom = expanded_and_clipped_geometry(shape, margin, output);
        assert_eq!(geom.loc.x, 76);
        assert_eq!(geom.loc.y, 76);
        assert_eq!(geom.size.w, 248);
        assert_eq!(geom.size.h, 198);

        // 2. Shape at top-left origin gets clipped at 0, 0
        let shape_tl = Rectangle::new((0, 0).into(), (100, 100).into());
        let geom_tl = expanded_and_clipped_geometry(shape_tl, margin, output);
        assert_eq!(geom_tl.loc.x, 0);
        assert_eq!(geom_tl.loc.y, 0);
        assert_eq!(geom_tl.size.w, 124);
        assert_eq!(geom_tl.size.h, 124);

        // 3. Shape at bottom-right gets clipped at output dimensions
        let shape_br = Rectangle::new((1850, 1020).into(), (70, 60).into());
        let geom_br = expanded_and_clipped_geometry(shape_br, margin, output);
        assert_eq!(geom_br.loc.x, 1826);
        assert_eq!(geom_br.loc.y, 996);
        assert_eq!(geom_br.loc.x + geom_br.size.w, 1920);
        assert_eq!(geom_br.loc.y + geom_br.size.h, 1080);
    }
}
