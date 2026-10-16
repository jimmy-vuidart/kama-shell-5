//! Glass: what is behind an element, blurred (Dual Kawase), in a shape.
//!
//! A [`GlassElement`] is a framebuffer effect: Smithay calls
//! `capture_framebuffer` before `draw` whenever what is behind it changed. The
//! capture copies that part of the render target into a texture and blurs it;
//! `draw` shows the blurred texture through a mask: a rounded rectangle (the
//! shell's decorations), or the alpha of a client surface (the shell's own
//! programs).
//!
//! Everything is placed from `gl_FragCoord`, through the renderer's own
//! projection: no guess about which way up the render target is.

use std::sync::Mutex;

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::{
    Frame, FrameContext, Offscreen, Texture,
    element::{Element, Id, Kind, RenderElement, UnderlyingStorage},
    gles::{GlesError, GlesFrame, GlesRenderer, GlesTexture, Uniform, ffi},
    utils::{CommitCounter, DamageSet, OpaqueRegions},
};
use smithay::utils::{
    Buffer as BufferCoord, Physical, Rectangle, Scale, Size, Transform, user_data::UserDataMap,
};
use tracing::warn;

use super::clip::frag_to_local;
use super::shaders::{BlurProgram, Shaders};

/// Which part of the blurred background shows.
#[derive(Debug, Clone)]
pub enum Mask {
    /// A rounded rectangle: top-left, top-right, bottom-right, bottom-left.
    Rounded { radii: [f32; 4] },
    /// Where a client surface is opaque enough, cut by optional rounded
    /// corners.
    Alpha {
        /// The surface's texture, drawn by the renderer itself: it knows
        /// external, y-inverted and cropped buffers.
        texture: GlesTexture,
        src: Rectangle<f64, BufferCoord>,
        /// Where the surface is drawn, in physical output coordinates.
        dst: Rectangle<i32, Physical>,
        transform: Transform,
        /// The surface's commit: the mask is drawn again when it changes.
        commit: CommitCounter,
        radii: [f32; 4],
    },
}

impl Mask {
    /// What the shape of the glass depends on. A new buffer of the same
    /// surface does not change it: only the blurred background is costly,
    /// and it depends on what is behind.
    fn key(&self) -> MaskKey {
        match self {
            Mask::Rounded { radii } => MaskKey::Rounded(*radii),
            Mask::Alpha { radii, .. } => MaskKey::Alpha(*radii),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum MaskKey {
    Rounded([f32; 4]),
    Alpha([f32; 4]),
}

/// Clamp blur strength to 1..=5.
pub fn clamp_strength(strength: u8) -> u8 {
    strength.clamp(1, 5)
}

/// How far a pass samples, in texels of its input times its half-pixel: the
/// same for both directions, a little more at each level.
pub fn pass_offset(strength: u8, level: usize) -> f32 {
    1.0 + f32::from(clamp_strength(strength)) * 0.15 + level as f32 * 0.25
}

/// How far, in pixels of the screen, the blur reaches: what a pixel of the
/// glass may take from around it. Down pass `i` reads half its offset away in
/// texels of level `i` (2^i pixels); up pass `i` reads its whole offset away
/// in texels of level `i + 1`.
pub fn blur_reach(strength: u8) -> i32 {
    let passes = usize::from(clamp_strength(strength));
    let reach: f32 = (0..passes)
        .map(|level| {
            let scale = (1u32 << level) as f32;
            let offset = pass_offset(strength, level);
            0.5 * offset * scale + offset * 2.0 * scale
        })
        .sum();
    reach.ceil() as i32
}

/// Margin captured around the shape: the reach of the blur, so that the
/// edges of the glass blur what is really there instead of a stretched
/// border.
pub fn blur_margin(strength: u8) -> i32 {
    blur_reach(strength)
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

/// Where `rect`, in physical output coordinates, lands in the render target:
/// GL window coordinates (origin at the bottom left), clipped to the target.
///
/// `projection` is the renderer's column-major matrix from output coordinates
/// to clip space, `target` the size of the render target.
pub fn window_rect(
    projection: &[f32; 9],
    target: Size<i32, Physical>,
    rect: Rectangle<i32, Physical>,
) -> Option<Rectangle<i32, Physical>> {
    let [a, b, _, c, d, _, e, f, _] = *projection;
    let window = |x: f32, y: f32| {
        let (nx, ny) = (a * x + c * y + e, b * x + d * y + f);
        (
            (nx + 1.0) * 0.5 * target.w as f32,
            (ny + 1.0) * 0.5 * target.h as f32,
        )
    };
    let (x0, y0) = (rect.loc.x as f32, rect.loc.y as f32);
    let (x1, y1) = (x0 + rect.size.w as f32, y0 + rect.size.h as f32);
    let (p, q) = (window(x0, y0), window(x1, y1));
    let left = (p.0.min(q.0).round() as i32).max(0);
    let right = (p.0.max(q.0).round() as i32).min(target.w);
    let bottom = (p.1.min(q.1).round() as i32).max(0);
    let top = (p.1.max(q.1).round() as i32).min(target.h);
    (right > left && top > bottom)
        .then(|| Rectangle::new((left, bottom).into(), (right - left, top - bottom).into()))
}

/// The sizes of the downsampled levels of a blur, each half the one before.
pub fn level_sizes(size: Size<i32, BufferCoord>, passes: usize) -> Vec<Size<i32, BufferCoord>> {
    let mut sizes = Vec::with_capacity(passes);
    let mut current = size;
    for _ in 0..passes {
        current = (((current.w + 1) / 2).max(1), ((current.h + 1) / 2).max(1)).into();
        sizes.push(current);
    }
    sizes
}

/// Textures are allocated by steps of this many pixels, so that an element
/// that grows or shrinks a little (a dock that unfolds, a window resized)
/// keeps its textures.
const ALLOCATION_STEP: i32 = 64;

/// The size to allocate for `need` pixels.
pub fn allocation(need: Size<i32, BufferCoord>) -> Size<i32, BufferCoord> {
    let step = |n: i32| (n.max(1) + ALLOCATION_STEP - 1) / ALLOCATION_STEP * ALLOCATION_STEP;
    (step(need.w), step(need.h)).into()
}

/// Whether a texture of size `allocated` can hold `need` pixels without
/// wasting too much memory.
pub fn fits(allocated: Size<i32, BufferCoord>, need: Size<i32, BufferCoord>) -> bool {
    let fits = |have: i32, need: i32| have >= need && have <= 2 * need + ALLOCATION_STEP;
    fits(allocated.w, need.w) && fits(allocated.h, need.h)
}

/// A render element that captures and blurs what is behind it.
#[derive(Debug, Clone)]
pub struct GlassElement {
    id: Id,
    commit: CommitCounter,
    shaders: Shaders,
    shape: Rectangle<i32, Physical>,
    geometry: Rectangle<i32, Physical>,
    mask: Mask,
    strength: u8,
}

impl GlassElement {
    fn new(
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
            mask,
            strength,
        }
    }
}

/// Keeps an element's Id and commit counter between frames.
#[derive(Debug)]
pub struct GlassCache {
    id: Id,
    commit: CommitCounter,
    last_spec: Option<(Rectangle<i32, Physical>, MaskKey, u8)>,
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
        let spec = (shape, mask.key(), strength);
        if self.last_spec != Some(spec) {
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

/// GL objects shared by every glass element of a renderer: one framebuffer
/// that each pass attaches its texture to, and the quad the passes draw.
/// They live as long as the GL context.
#[derive(Debug, Clone, Copy)]
struct SharedGl {
    fbo: u32,
    vbo: u32,
}

/// A texture of a blur, larger than what it holds.
#[derive(Debug)]
struct Level {
    texture: GlesTexture,
    /// Pixels in use, from the bottom left corner.
    used: Size<i32, BufferCoord>,
}

impl Level {
    fn allocated(&self) -> Size<i32, BufferCoord> {
        self.texture.size()
    }

    fn pass(&self) -> PassTexture {
        let allocated = self.allocated();
        PassTexture {
            id: self.texture.tex_id(),
            used: (self.used.w, self.used.h),
            allocated: (allocated.w, allocated.h),
        }
    }
}

/// What a blur pass needs to know of a texture, copied out of the cache.
#[derive(Debug, Clone, Copy)]
struct PassTexture {
    id: u32,
    used: (i32, i32),
    allocated: (i32, i32),
}

/// What a mask was drawn from: the surface's commit, the glass's shape and
/// where the surface was.
type MaskDrawn = (
    CommitCounter,
    Rectangle<i32, Physical>,
    Rectangle<i32, Physical>,
);

/// What an element keeps between frames, per render target.
#[derive(Debug, Default)]
struct BlurCache {
    /// The captured background, then, after the passes, the blurred one.
    source: Option<Level>,
    down: Vec<Option<Level>>,
    /// Where `source` was taken from, in GL window coordinates.
    origin: (i32, i32),
    /// Whether `source` holds a blurred background ready to be drawn.
    ready: bool,
    /// The alpha of the client surface, for `Mask::Alpha`.
    mask: Option<Level>,
    mask_origin: (i32, i32),
    /// What `mask` was drawn from: the surface's commit, and the place.
    mask_drawn: Option<MaskDrawn>,
    /// A GL error was already reported for this element.
    reported: bool,
}

/// Uses `slot` for `need` pixels, allocating a texture when it cannot hold
/// them.
fn ensure(
    frame: &mut GlesFrame<'_, '_>,
    slot: &mut Option<Level>,
    need: Size<i32, BufferCoord>,
) -> Result<(), GlesError> {
    if let Some(level) = slot.as_mut().filter(|level| fits(level.allocated(), need)) {
        level.used = need;
        return Ok(());
    }
    // The guard gives the renderer back to the frame when dropped, binding
    // its render target again.
    let texture = {
        let mut guard = FrameContext::renderer(frame);
        Offscreen::<GlesTexture>::create_buffer(guard.as_mut(), Fourcc::Abgr8888, allocation(need))?
    };
    *slot = Some(Level {
        texture,
        used: need,
    });
    Ok(())
}

/// The GL objects every glass element shares, made the first time.
// Unsafe: GL calls through the renderer's context, which Smithay hands over
// for this; the buffer binding they change is put back.
#[allow(unsafe_code)]
fn shared_gl(frame: &mut GlesFrame<'_, '_>) -> Result<SharedGl, GlesError> {
    if let Some(shared) = frame.egl_context().user_data().get::<SharedGl>() {
        return Ok(*shared);
    }
    let shared = frame.with_context(|gl| unsafe {
        let mut fbo = 0;
        gl.GenFramebuffers(1, &mut fbo);
        let mut previous = 0;
        gl.GetIntegerv(ffi::ARRAY_BUFFER_BINDING, &mut previous);
        let mut vbo = 0;
        gl.GenBuffers(1, &mut vbo);
        gl.BindBuffer(ffi::ARRAY_BUFFER, vbo);
        static QUAD: [f32; 8] = [-1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0];
        gl.BufferData(
            ffi::ARRAY_BUFFER,
            std::mem::size_of_val(&QUAD) as isize,
            QUAD.as_ptr() as *const _,
            ffi::STATIC_DRAW,
        );
        gl.BindBuffer(ffi::ARRAY_BUFFER, previous as u32);
        SharedGl { fbo, vbo }
    })?;
    frame.egl_context().user_data().insert_if_missing(|| shared);
    Ok(shared)
}

/// The framebuffer the frame draws into, and its size, as the GL context
/// has them now. Pending GL errors are cleared, so that the ones checked
/// afterwards are ours.
// Unsafe: read-only GL queries.
#[allow(unsafe_code)]
fn current_target(frame: &mut GlesFrame<'_, '_>) -> Result<(u32, Size<i32, Physical>), GlesError> {
    frame.with_context(|gl| unsafe {
        while gl.GetError() != ffi::NO_ERROR {}
        let mut fbo = 0;
        gl.GetIntegerv(ffi::DRAW_FRAMEBUFFER_BINDING, &mut fbo);
        let mut viewport = [0; 4];
        gl.GetIntegerv(ffi::VIEWPORT, viewport.as_mut_ptr());
        (fbo as u32, Size::from((viewport[2], viewport[3])))
    })
}

/// Reports the first GL error of an element; the following ones would only
/// repeat it.
// Unsafe: read-only GL query.
#[allow(unsafe_code)]
fn check_errors(frame: &mut GlesFrame<'_, '_>, cache: &mut BlurCache, what: &str) -> bool {
    let error = frame
        .with_context(|gl| unsafe {
            let first = gl.GetError();
            while gl.GetError() != ffi::NO_ERROR {}
            first
        })
        .unwrap_or(ffi::NO_ERROR);
    if error != ffi::NO_ERROR && !cache.reported {
        warn!(error, what, "glass: GL error, this element shows no blur");
        cache.reported = true;
    }
    error == ffi::NO_ERROR
}

impl GlassElement {
    /// Copies what is behind the element and blurs it into `cache.source`.
    // Unsafe: raw GL calls, which Smithay does not offer: copying part of the
    // render target (glBlitFramebuffer) and drawing the blur passes. Every
    // piece of GL state they change is put back before returning: framebuffer
    // bindings, viewport, scissor, blending, program, buffer, texture unit
    // and texture, vertex attribute.
    #[allow(unsafe_code)]
    fn capture(
        &self,
        frame: &mut GlesFrame<'_, '_>,
        dst: Rectangle<i32, Physical>,
        cache: &mut BlurCache,
    ) -> Result<(), GlesError> {
        cache.ready = false;
        let shared = shared_gl(frame)?;
        let (target_fbo, target) = current_target(frame)?;
        let Some(region) = window_rect(frame.projection(), target, dst) else {
            return Ok(());
        };
        let size: Size<i32, BufferCoord> = (region.size.w, region.size.h).into();
        let passes = clamp_strength(self.strength) as usize;

        ensure(frame, &mut cache.source, size)?;
        cache.down.resize_with(passes, || None);
        for (slot, need) in cache.down.iter_mut().zip(level_sizes(size, passes)) {
            ensure(frame, slot, need)?;
        }
        let source = cache.source.as_ref().map(Level::pass);
        let down: Option<Vec<PassTexture>> = cache
            .down
            .iter()
            .map(|slot| slot.as_ref().map(Level::pass))
            .collect();
        let (Some(source), Some(down)) = (source, down) else {
            return Ok(());
        };

        let down_program = self.shaders.blur_down;
        let up_program = self.shaders.blur_up;
        let strength = self.strength;

        frame.with_context(|gl| unsafe {
            let mut draw_fbo = 0;
            gl.GetIntegerv(ffi::DRAW_FRAMEBUFFER_BINDING, &mut draw_fbo);
            let mut read_fbo = 0;
            gl.GetIntegerv(ffi::READ_FRAMEBUFFER_BINDING, &mut read_fbo);
            let mut viewport = [0; 4];
            gl.GetIntegerv(ffi::VIEWPORT, viewport.as_mut_ptr());
            let mut program = 0;
            gl.GetIntegerv(ffi::CURRENT_PROGRAM, &mut program);
            let mut array_buffer = 0;
            gl.GetIntegerv(ffi::ARRAY_BUFFER_BINDING, &mut array_buffer);
            let mut active_texture = 0;
            gl.GetIntegerv(ffi::ACTIVE_TEXTURE, &mut active_texture);
            gl.ActiveTexture(ffi::TEXTURE0);
            let mut texture0 = 0;
            gl.GetIntegerv(ffi::TEXTURE_BINDING_2D, &mut texture0);
            let scissor = gl.IsEnabled(ffi::SCISSOR_TEST) == ffi::TRUE;
            let blend = gl.IsEnabled(ffi::BLEND) == ffi::TRUE;

            gl.Disable(ffi::SCISSOR_TEST);
            gl.Disable(ffi::BLEND);

            // 1. What is behind, as it is in the render target now.
            gl.BindFramebuffer(ffi::READ_FRAMEBUFFER, target_fbo);
            gl.BindFramebuffer(ffi::DRAW_FRAMEBUFFER, shared.fbo);
            gl.FramebufferTexture2D(
                ffi::DRAW_FRAMEBUFFER,
                ffi::COLOR_ATTACHMENT0,
                ffi::TEXTURE_2D,
                source.id,
                0,
            );
            gl.BlitFramebuffer(
                region.loc.x,
                region.loc.y,
                region.loc.x + region.size.w,
                region.loc.y + region.size.h,
                0,
                0,
                region.size.w,
                region.size.h,
                ffi::COLOR_BUFFER_BIT,
                ffi::NEAREST,
            );

            // 2. Down to the smallest level, then back up into `source`.
            gl.BindFramebuffer(ffi::FRAMEBUFFER, shared.fbo);
            gl.BindBuffer(ffi::ARRAY_BUFFER, shared.vbo);
            let run =
                |program: BlurProgram, input: PassTexture, output: PassTexture, offset: f32| {
                    gl.FramebufferTexture2D(
                        ffi::FRAMEBUFFER,
                        ffi::COLOR_ATTACHMENT0,
                        ffi::TEXTURE_2D,
                        output.id,
                        0,
                    );
                    gl.Viewport(0, 0, output.used.0, output.used.1);
                    gl.BindTexture(ffi::TEXTURE_2D, input.id);
                    for (name, value) in [
                        (ffi::TEXTURE_MIN_FILTER, ffi::LINEAR),
                        (ffi::TEXTURE_MAG_FILTER, ffi::LINEAR),
                        (ffi::TEXTURE_WRAP_S, ffi::CLAMP_TO_EDGE),
                        (ffi::TEXTURE_WRAP_T, ffi::CLAMP_TO_EDGE),
                    ] {
                        gl.TexParameteri(ffi::TEXTURE_2D, name, value as i32);
                    }
                    let (aw, ah) = (input.allocated.0 as f32, input.allocated.1 as f32);
                    let (uw, uh) = (input.used.0 as f32, input.used.1 as f32);
                    gl.UseProgram(program.program);
                    gl.Uniform1i(program.u_tex, 0);
                    gl.Uniform2f(program.u_halfpixel, 0.5 / aw, 0.5 / ah);
                    gl.Uniform1f(program.u_offset, offset);
                    gl.Uniform2f(program.u_uv_scale, uw / aw, uh / ah);
                    gl.Uniform2f(program.u_uv_max, (uw - 0.5) / aw, (uh - 0.5) / ah);
                    gl.EnableVertexAttribArray(program.a_position);
                    gl.VertexAttribPointer(
                        program.a_position,
                        2,
                        ffi::FLOAT,
                        ffi::FALSE,
                        0,
                        std::ptr::null(),
                    );
                    gl.DrawArrays(ffi::TRIANGLE_STRIP, 0, 4);
                    gl.DisableVertexAttribArray(program.a_position);
                };
            let mut input = source;
            for (index, level) in down.iter().enumerate() {
                run(down_program, input, *level, pass_offset(strength, index));
                input = *level;
            }
            for index in (0..down.len()).rev() {
                let output = if index == 0 { source } else { down[index - 1] };
                run(
                    up_program,
                    down[index],
                    output,
                    pass_offset(strength, index),
                );
            }

            // Leave nothing attached: the textures may be freed at any time.
            gl.FramebufferTexture2D(
                ffi::FRAMEBUFFER,
                ffi::COLOR_ATTACHMENT0,
                ffi::TEXTURE_2D,
                0,
                0,
            );
            gl.BindTexture(ffi::TEXTURE_2D, texture0 as u32);
            gl.ActiveTexture(active_texture as u32);
            gl.BindBuffer(ffi::ARRAY_BUFFER, array_buffer as u32);
            gl.UseProgram(program as u32);
            gl.BindFramebuffer(ffi::DRAW_FRAMEBUFFER, draw_fbo as u32);
            gl.BindFramebuffer(ffi::READ_FRAMEBUFFER, read_fbo as u32);
            gl.Viewport(viewport[0], viewport[1], viewport[2], viewport[3]);
            if scissor {
                gl.Enable(ffi::SCISSOR_TEST);
            }
            if blend {
                gl.Enable(ffi::BLEND);
            }
        })?;

        cache.origin = (region.loc.x, region.loc.y);
        cache.ready = check_errors(frame, cache, "capture");
        Ok(())
    }

    /// Draws the client surface of a `Mask::Alpha` into `cache.mask`, at the
    /// place it has in the render target, when it changed.
    // Unsafe: GL calls that point the frame at the mask texture for one draw
    // made by the renderer itself, then point it back: framebuffer bindings,
    // viewport and scissor are put back as they were.
    #[allow(unsafe_code)]
    fn draw_mask(
        &self,
        frame: &mut GlesFrame<'_, '_>,
        cache: &mut BlurCache,
    ) -> Result<bool, GlesError> {
        let Mask::Alpha {
            texture,
            src,
            dst,
            transform,
            commit,
            ..
        } = &self.mask
        else {
            return Ok(true);
        };
        let drawn = (*commit, self.shape, *dst);
        if cache.mask.is_some() && cache.mask_drawn == Some(drawn) {
            return Ok(true);
        }
        cache.mask_drawn = None;
        let shared = shared_gl(frame)?;
        let (target_fbo, target) = current_target(frame)?;
        let Some(region) = window_rect(frame.projection(), target, self.shape) else {
            return Ok(false);
        };
        ensure(
            frame,
            &mut cache.mask,
            (region.size.w, region.size.h).into(),
        )?;
        let Some(mask) = cache.mask.as_ref().map(Level::pass) else {
            return Ok(false);
        };

        let saved = frame.with_context(|gl| unsafe {
            let mut read_fbo = 0;
            gl.GetIntegerv(ffi::READ_FRAMEBUFFER_BINDING, &mut read_fbo);
            let mut viewport = [0; 4];
            gl.GetIntegerv(ffi::VIEWPORT, viewport.as_mut_ptr());
            let scissor = gl.IsEnabled(ffi::SCISSOR_TEST) == ffi::TRUE;

            gl.BindFramebuffer(ffi::FRAMEBUFFER, shared.fbo);
            gl.FramebufferTexture2D(
                ffi::FRAMEBUFFER,
                ffi::COLOR_ATTACHMENT0,
                ffi::TEXTURE_2D,
                mask.id,
                0,
            );
            gl.Disable(ffi::SCISSOR_TEST);
            gl.ClearColor(0.0, 0.0, 0.0, 0.0);
            gl.Clear(ffi::COLOR_BUFFER_BIT);
            // The whole target, moved so that the shape's corner is at the
            // texture's corner: the surface lands where it is on screen.
            gl.Viewport(-region.loc.x, -region.loc.y, target.w, target.h);
            (read_fbo as u32, viewport, scissor)
        })?;

        let drawn_ok = frame.render_texture_from_to(
            texture,
            *src,
            *dst,
            &[Rectangle::from_size(dst.size)],
            &[],
            *transform,
            1.0,
            None,
            &[],
        );

        frame.with_context(|gl| unsafe {
            let (read_fbo, viewport, scissor) = saved;
            gl.FramebufferTexture2D(
                ffi::FRAMEBUFFER,
                ffi::COLOR_ATTACHMENT0,
                ffi::TEXTURE_2D,
                0,
                0,
            );
            gl.BindFramebuffer(ffi::DRAW_FRAMEBUFFER, target_fbo);
            gl.BindFramebuffer(ffi::READ_FRAMEBUFFER, read_fbo);
            gl.Viewport(viewport[0], viewport[1], viewport[2], viewport[3]);
            if scissor {
                gl.Enable(ffi::SCISSOR_TEST);
            }
        })?;

        let ok = drawn_ok.is_ok() && check_errors(frame, cache, "mask");
        if ok {
            cache.mask_origin = (region.loc.x, region.loc.y);
            cache.mask_drawn = Some(drawn);
        }
        Ok(ok)
    }

    /// Draws the blurred background through the mask.
    // Unsafe: binds the mask texture to texture unit 1 for the glass shader,
    // then unbinds it and makes unit 0 active again.
    #[allow(unsafe_code)]
    fn draw_glass(
        &self,
        frame: &mut GlesFrame<'_, '_>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        cache: &mut BlurCache,
    ) -> Result<(), GlesError> {
        let Some(source) = cache.source.as_ref().filter(|_| cache.ready) else {
            return Ok(());
        };
        let alpha_mask = matches!(self.mask, Mask::Alpha { .. });
        let mask = cache.mask.as_ref().filter(|_| alpha_mask);
        if alpha_mask && mask.is_none() {
            return Ok(());
        }

        let viewport = frame.output_size();
        let (row_x, row_y) = frag_to_local(
            frame.projection(),
            [viewport.w as f32, viewport.h as f32],
            [self.shape.loc.x as f32, self.shape.loc.y as f32],
        );
        let radii = match &self.mask {
            Mask::Rounded { radii } | Mask::Alpha { radii, .. } => *radii,
        };
        let texel = |level: &Level| {
            let allocated = level.allocated();
            [1.0 / allocated.w as f32, 1.0 / allocated.h as f32]
        };
        let limit = |level: &Level| {
            let allocated = level.allocated();
            [
                (level.used.w as f32 - 0.5) / allocated.w as f32,
                (level.used.h as f32 - 0.5) / allocated.h as f32,
            ]
        };
        let origin = |(x, y): (i32, i32)| [x as f32, y as f32];
        let uniforms = vec![
            Uniform::new("clip_row_x", row_x),
            Uniform::new("clip_row_y", row_y),
            Uniform::new(
                "shape_size",
                [self.shape.size.w as f32, self.shape.size.h as f32],
            ),
            Uniform::new("radii", radii),
            Uniform::new("mask_mode", i32::from(alpha_mask)),
            Uniform::new("mask_tex", 1i32),
            Uniform::new("saturation", 1.15f32),
            Uniform::new("blur_origin", origin(cache.origin)),
            Uniform::new("blur_texel", texel(source)),
            Uniform::new("blur_max", limit(source)),
            Uniform::new("mask_origin", origin(cache.mask_origin)),
            Uniform::new("mask_texel", mask.map_or([0.0; 2], texel)),
            Uniform::new("mask_max", mask.map_or([0.0; 2], limit)),
        ];

        let mask_id = mask.map(|mask| mask.texture.tex_id());
        if let Some(id) = mask_id {
            frame.with_context(|gl| unsafe {
                gl.ActiveTexture(ffi::TEXTURE1);
                gl.BindTexture(ffi::TEXTURE_2D, id);
                // A texture the renderer never sampled keeps GL's default
                // filter, which wants mipmaps: without them it reads as
                // opaque everywhere.
                for (name, value) in [
                    (ffi::TEXTURE_MIN_FILTER, ffi::NEAREST),
                    (ffi::TEXTURE_MAG_FILTER, ffi::NEAREST),
                    (ffi::TEXTURE_WRAP_S, ffi::CLAMP_TO_EDGE),
                    (ffi::TEXTURE_WRAP_T, ffi::CLAMP_TO_EDGE),
                ] {
                    gl.TexParameteri(ffi::TEXTURE_2D, name, value as i32);
                }
                gl.ActiveTexture(ffi::TEXTURE0);
            })?;
        }
        let result = frame.render_texture_from_to(
            &source.texture,
            Rectangle::from_size((source.used.w as f64, source.used.h as f64).into()),
            dst,
            damage,
            &[],
            Transform::Normal,
            1.0,
            Some(&self.shaders.glass),
            &uniforms,
        );
        if mask_id.is_some() {
            frame.with_context(|gl| unsafe {
                gl.ActiveTexture(ffi::TEXTURE1);
                gl.BindTexture(ffi::TEXTURE_2D, 0);
                gl.ActiveTexture(ffi::TEXTURE0);
            })?;
        }
        result
    }
}

impl RenderElement<GlesRenderer> for GlassElement {
    fn capture_framebuffer(
        &self,
        frame: &mut GlesFrame<'_, '_>,
        _src: Rectangle<f64, BufferCoord>,
        dst: Rectangle<i32, Physical>,
        cache: &UserDataMap,
    ) -> Result<(), GlesError> {
        let cell = cache.get_or_insert_threadsafe(|| Mutex::new(BlurCache::default()));
        let mut blur = cell.lock().unwrap();
        // A failure leaves this element without blur, never the frame
        // without the rest of the scene.
        if let Err(error) = self.capture(frame, dst, &mut blur) {
            blur.ready = false;
            if !blur.reported {
                warn!(?error, "glass: cannot blur the background");
                blur.reported = true;
            }
        }
        Ok(())
    }

    fn draw(
        &self,
        frame: &mut GlesFrame<'_, '_>,
        _src: Rectangle<f64, BufferCoord>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        _opaque_regions: &[Rectangle<i32, Physical>],
        cache: Option<&UserDataMap>,
    ) -> Result<(), GlesError> {
        let Some(cell) = cache.and_then(|cache| cache.get::<Mutex<BlurCache>>()) else {
            return Ok(());
        };
        let mut blur = cell.lock().unwrap();
        let result = match self.draw_mask(frame, &mut blur) {
            Ok(true) => self.draw_glass(frame, dst, damage, &mut blur),
            Ok(false) => Ok(()),
            Err(error) => Err(error),
        };
        if let Err(error) = result {
            if !blur.reported {
                warn!(?error, "glass: cannot draw");
                blur.reported = true;
            }
        }
        Ok(())
    }

    fn underlying_storage(&self, _renderer: &mut GlesRenderer) -> Option<UnderlyingStorage<'_>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: f32 = 1920.0;
    const H: f32 = 1080.0;
    /// Output drawn upright, as on the hardware.
    const NORMAL: [f32; 9] = [2.0 / W, 0.0, 0.0, 0.0, -2.0 / H, 0.0, -1.0, 1.0, 1.0];
    /// What the winit backend uses: the output is flipped vertically.
    const FLIPPED: [f32; 9] = [2.0 / W, 0.0, 0.0, 0.0, 2.0 / H, 0.0, -1.0, -1.0, 1.0];

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rectangle<i32, Physical> {
        Rectangle::new((x, y).into(), (w, h).into())
    }

    fn target() -> Size<i32, Physical> {
        (W as i32, H as i32).into()
    }

    #[test]
    fn strength_is_clamped_to_one_to_five() {
        assert_eq!(clamp_strength(0), 1);
        assert_eq!(clamp_strength(3), 3);
        assert_eq!(clamp_strength(100), 5);
    }

    #[test]
    fn the_margin_covers_what_the_blur_reads() {
        // Strength 3: down 0.5 × (1.45 + 1.7×2 + 1.95×4), up 2 × the same.
        assert_eq!(blur_reach(3), 32);
        for strength in 1..=5 {
            assert!(blur_margin(strength) >= blur_reach(strength));
        }
        assert!(blur_margin(1) < blur_margin(3) && blur_margin(3) < blur_margin(5));
        assert_eq!(blur_margin(10), blur_margin(5), "clamped like the strength");
    }

    #[test]
    fn geometry_expansion_and_clipping_to_output() {
        let output: Size<i32, Physical> = (1920, 1080).into();
        let geom = expanded_and_clipped_geometry(rect(100, 100, 200, 150), 24, output);
        assert_eq!(geom, rect(76, 76, 248, 198));
        let geom = expanded_and_clipped_geometry(rect(0, 0, 100, 100), 24, output);
        assert_eq!(geom, rect(0, 0, 124, 124));
        let geom = expanded_and_clipped_geometry(rect(1850, 1020, 70, 60), 24, output);
        assert_eq!(geom, rect(1826, 996, 94, 84));
    }

    #[test]
    fn an_upright_output_puts_the_top_of_the_screen_at_the_top_of_the_target() {
        // 100 px from the top of the screen is 1080 - 100 - 50 from the bottom.
        let window = window_rect(&NORMAL, target(), rect(10, 100, 200, 50)).unwrap();
        assert_eq!(window, rect(10, 930, 200, 50));
    }

    #[test]
    fn a_flipped_output_keeps_screen_rows_as_target_rows() {
        let window = window_rect(&FLIPPED, target(), rect(10, 100, 200, 50)).unwrap();
        assert_eq!(window, rect(10, 100, 200, 50));
    }

    #[test]
    fn window_rect_is_clipped_to_the_target() {
        let window = window_rect(&FLIPPED, target(), rect(-20, 1060, 100, 100)).unwrap();
        assert_eq!(window, rect(0, 1060, 80, 20));
        assert_eq!(window_rect(&FLIPPED, target(), rect(2000, 0, 10, 10)), None);
    }

    #[test]
    fn levels_halve_and_never_vanish() {
        let sizes = level_sizes((101, 3).into(), 3);
        let sizes: Vec<(i32, i32)> = sizes.iter().map(|size| (size.w, size.h)).collect();
        assert_eq!(sizes, [(51, 2), (26, 1), (13, 1)]);
    }

    #[test]
    fn small_changes_of_size_keep_the_texture() {
        let allocated = allocation((260, 472).into());
        assert_eq!((allocated.w, allocated.h), (320, 512));
        assert!(fits(allocated, (260, 472).into()));
        assert!(
            fits(allocated, (300, 500).into()),
            "a dock that unfolds a little"
        );
        assert!(!fits(allocated, (330, 472).into()), "too small");
        assert!(!fits(allocated, (40, 40).into()), "too wasteful");
    }
}
