//! Custom GLES programs.

use smithay::backend::renderer::gles::{
    GlesError, GlesPixelProgram, GlesRenderer, GlesTexProgram, UniformName, UniformType, ffi,
};

/// A compiled raw GL shader program for Dual Kawase blur passes.
#[derive(Clone, Copy, Debug)]
pub struct BlurProgram {
    pub program: u32,
    pub u_tex: i32,
    pub u_halfpixel: i32,
    pub u_offset: i32,
    /// Part of the input texture in use: blur textures are larger than
    /// what they hold.
    pub u_uv_scale: i32,
    pub u_uv_max: i32,
    pub a_position: u32,
}

#[derive(Clone, Debug)]
pub struct Shaders {
    /// Rounded rectangle, filled or ring: accent ring, window background.
    pub rounded: GlesPixelProgram,
    /// Draws a surface clipped to a rounded rectangle.
    pub clip: GlesTexProgram,
    /// Glass effect shader (masking, saturation, dithering).
    pub glass: GlesTexProgram,
    /// Gaussian soft shadow for rounded rectangles.
    pub shadow: GlesPixelProgram,
    /// Raw GL program for Dual Kawase downsampling.
    pub blur_down: BlurProgram,
    /// Raw GL program for Dual Kawase upsampling.
    pub blur_up: BlurProgram,
}

const BLUR_VERT: &str = r#"
attribute vec2 position;
varying vec2 v_uv;
void main() {
    v_uv = position * 0.5 + 0.5;
    gl_Position = vec4(position, 0.0, 1.0);
}
"#;

// Documented unsafe rationale: interacting directly with OpenGL ES C-FFI
// for shader compilation not exposed by Smithay.
#[allow(unsafe_code)]
fn compile_shader(gl: &ffi::Gles2, stage: u32, source: &str) -> Result<u32, GlesError> {
    unsafe {
        let shader = gl.CreateShader(stage);
        let c_str = match std::ffi::CString::new(source) {
            Ok(c) => c,
            Err(_) => {
                tracing::error!("invalid shader source string");
                return Err(GlesError::ShaderCompileError);
            }
        };
        gl.ShaderSource(shader, 1, &c_str.as_ptr(), std::ptr::null());
        gl.CompileShader(shader);
        let mut status = 0;
        gl.GetShaderiv(shader, ffi::COMPILE_STATUS, &mut status);
        if status == 0 {
            let mut len = 0;
            gl.GetShaderiv(shader, ffi::INFO_LOG_LENGTH, &mut len);
            let mut buffer = vec![0u8; len as usize];
            gl.GetShaderInfoLog(
                shader,
                len,
                std::ptr::null_mut(),
                buffer.as_mut_ptr() as *mut _,
            );
            gl.DeleteShader(shader);
            let msg = String::from_utf8_lossy(&buffer).to_string();
            tracing::error!(%msg, "shader compile error");
            return Err(GlesError::ShaderCompileError);
        }
        Ok(shader)
    }
}

// Documented unsafe rationale: interacting directly with OpenGL ES C-FFI
// for program linking and uniform location querying.
#[allow(unsafe_code)]
fn link_blur_program(
    gl: &ffi::Gles2,
    vert_shader: u32,
    frag_source: &str,
) -> Result<BlurProgram, GlesError> {
    let frag_shader = compile_shader(gl, ffi::FRAGMENT_SHADER, frag_source)?;
    unsafe {
        let program = gl.CreateProgram();
        gl.AttachShader(program, vert_shader);
        gl.AttachShader(program, frag_shader);
        gl.LinkProgram(program);

        let mut status = 0;
        gl.GetProgramiv(program, ffi::LINK_STATUS, &mut status);
        if status == 0 {
            let mut len = 0;
            gl.GetProgramiv(program, ffi::INFO_LOG_LENGTH, &mut len);
            let mut buffer = vec![0u8; len as usize];
            gl.GetProgramInfoLog(
                program,
                len,
                std::ptr::null_mut(),
                buffer.as_mut_ptr() as *mut _,
            );
            gl.DeleteShader(frag_shader);
            gl.DeleteProgram(program);
            let msg = String::from_utf8_lossy(&buffer).to_string();
            tracing::error!(%msg, "blur program link error");
            return Err(GlesError::ShaderCompileError);
        }

        gl.DeleteShader(frag_shader);

        let c_tex = std::ffi::CString::new("tex").unwrap();
        let c_halfpixel = std::ffi::CString::new("halfpixel").unwrap();
        let c_offset = std::ffi::CString::new("offset").unwrap();
        let c_pos = std::ffi::CString::new("position").unwrap();
        let c_uv_scale = std::ffi::CString::new("uv_scale").unwrap();
        let c_uv_max = std::ffi::CString::new("uv_max").unwrap();

        let u_tex = gl.GetUniformLocation(program, c_tex.as_ptr());
        let u_halfpixel = gl.GetUniformLocation(program, c_halfpixel.as_ptr());
        let u_offset = gl.GetUniformLocation(program, c_offset.as_ptr());
        let a_position = gl.GetAttribLocation(program, c_pos.as_ptr()) as u32;
        let u_uv_scale = gl.GetUniformLocation(program, c_uv_scale.as_ptr());
        let u_uv_max = gl.GetUniformLocation(program, c_uv_max.as_ptr());

        Ok(BlurProgram {
            program,
            u_tex,
            u_halfpixel,
            u_offset,
            u_uv_scale,
            u_uv_max,
            a_position,
        })
    }
}

impl Shaders {
    // Documented unsafe rationale: raw GL calls are required to compile and link custom
    // fullscreen blur passes not exposed by Smithay's texture shader abstraction.
    #[allow(unsafe_code)]
    pub fn compile(renderer: &mut GlesRenderer) -> Result<Self, GlesError> {
        let rounded = renderer.compile_custom_pixel_shader(
            include_str!("shaders/rounded.frag"),
            &[
                UniformName::new("shape_size", UniformType::_2f),
                UniformName::new("shape_offset", UniformType::_2f),
                UniformName::new("color", UniformType::_4f),
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
        let glass = renderer.compile_custom_texture_shader(
            include_str!("shaders/glass.frag"),
            &[
                UniformName::new("clip_row_x", UniformType::_3f),
                UniformName::new("clip_row_y", UniformType::_3f),
                UniformName::new("shape_size", UniformType::_2f),
                UniformName::new("radii", UniformType::_4f),
                UniformName::new("mask_mode", UniformType::_1i),
                UniformName::new("mask_tex", UniformType::_1i),
                UniformName::new("saturation", UniformType::_1f),
                UniformName::new("blur_origin", UniformType::_2f),
                UniformName::new("blur_texel", UniformType::_2f),
                UniformName::new("blur_max", UniformType::_2f),
                UniformName::new("mask_origin", UniformType::_2f),
                UniformName::new("mask_texel", UniformType::_2f),
                UniformName::new("mask_max", UniformType::_2f),
            ],
        )?;
        let shadow = renderer.compile_custom_pixel_shader(
            include_str!("shaders/shadow.frag"),
            &[
                UniformName::new("shape_size", UniformType::_2f),
                UniformName::new("shape_offset", UniformType::_2f),
                UniformName::new("color", UniformType::_4f),
                UniformName::new("radius", UniformType::_4f),
                UniformName::new("blur", UniformType::_1f),
                UniformName::new("cutout", UniformType::_2f),
            ],
        )?;

        let (blur_down, blur_up) = renderer.with_context(|gl| unsafe {
            let vert = compile_shader(gl, ffi::VERTEX_SHADER, BLUR_VERT)?;
            let down = link_blur_program(gl, vert, include_str!("shaders/blur_down.frag"))?;
            let up = link_blur_program(gl, vert, include_str!("shaders/blur_up.frag"))?;
            gl.DeleteShader(vert);
            Ok::<_, GlesError>((down, up))
        })??;

        Ok(Self {
            rounded,
            clip,
            glass,
            shadow,
            blur_down,
            blur_up,
        })
    }
}
