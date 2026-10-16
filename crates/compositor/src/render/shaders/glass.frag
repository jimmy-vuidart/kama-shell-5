#version 100

//_DEFINES_

#if defined(EXTERNAL)
#extension GL_OES_EGL_image_external : require
#endif

precision highp float;
#if defined(EXTERNAL)
uniform samplerExternalOES tex;
#else
uniform sampler2D tex;
#endif

uniform float alpha;
varying vec2 v_coords;

#if defined(DEBUG_FLAGS)
uniform float tint;
#endif

// Affine map from gl_FragCoord to pixels of the glass rectangle.
uniform vec3 clip_row_x;
uniform vec3 clip_row_y;
uniform vec2 shape_size;
// Top-left, top-right, bottom-right, bottom-left.
uniform vec4 radii;

// Mask mode: 0 = rounded rectangle, 1 = alpha of a client surface.
uniform int mask_mode;
uniform sampler2D mask_tex;

// Saturation factor (e.g. 1.15)
uniform float saturation;

// Both textures were taken from the render target: their texel (0, 0) is the
// fragment at `*_origin`, and they hold more than they use, up to `*_max`.
uniform vec2 blur_origin;
uniform vec2 blur_texel;
uniform vec2 blur_max;
uniform vec2 mask_origin;
uniform vec2 mask_texel;
uniform vec2 mask_max;

float corner_radius(vec2 p) {
    if (p.x < 0.0) {
        return p.y < 0.0 ? radii.x : radii.w;
    }
    return p.y < 0.0 ? radii.y : radii.z;
}

float rand(vec2 co) {
    return fract(sin(dot(co.xy, vec2(12.9898, 78.233))) * 43758.5453);
}

vec3 adjust_saturation(vec3 rgb, float sat) {
    float luma = dot(rgb, vec3(0.2126, 0.7152, 0.0722));
    return mix(vec3(luma), rgb, sat);
}

// Coverage of the rounded rectangle at `p`, relative to its center.
float rounded(vec2 p, vec2 half_size) {
    float r = corner_radius(p);
    vec2 q = abs(p) - half_size + r;
    float dist = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
    return 1.0 - smoothstep(-0.5, 0.5, dist);
}

void main() {
    vec2 blur_uv = clamp((gl_FragCoord.xy - blur_origin) * blur_texel, vec2(0.0), blur_max);
    vec4 color = texture2D(tex, blur_uv);
#if defined(NO_ALPHA)
    color = vec4(color.rgb, 1.0);
#endif

    // Saturation boost, and a subtle grain against banding.
    float noise = (rand(gl_FragCoord.xy) - 0.5) * (1.5 / 255.0);
    color.rgb = clamp(adjust_saturation(color.rgb, saturation) + noise, 0.0, 1.0);

    vec3 frag = vec3(gl_FragCoord.xy, 1.0);
    vec2 local = vec2(dot(clip_row_x, frag), dot(clip_row_y, frag));
    vec2 half_size = shape_size * 0.5;
    vec2 p = local - half_size;

    float coverage = 0.0;
    if (mask_mode == 0) {
        coverage = rounded(p, half_size);
    } else {
        vec2 mask_uv = (gl_FragCoord.xy - mask_origin) * mask_texel;
        if (all(greaterThanEqual(mask_uv, vec2(0.0))) && all(lessThanEqual(mask_uv, mask_max))) {
            coverage = smoothstep(0.2, 0.4, texture2D(mask_tex, mask_uv).a);
            if (radii != vec4(0.0)) {
                coverage *= rounded(p, half_size);
            }
        }
    }

    color = color * coverage * alpha;

#if defined(DEBUG_FLAGS)
    if (tint == 1.0)
        color = vec4(0.0, 0.2, 0.0, 0.2) + color * 0.8;
#endif

    gl_FragColor = color;
}
