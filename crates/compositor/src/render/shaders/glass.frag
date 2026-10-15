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

// Mask mode: 0 = Rounded, 1 = Alpha
uniform int mask_mode;
uniform sampler2D mask_tex;

// Saturation factor (e.g. 1.15)
uniform float saturation;

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

void main() {
    vec4 color = texture2D(tex, v_coords);
#if defined(NO_ALPHA)
    color = vec4(color.rgb, 1.0);
#endif

    // Apply saturation boost and subtle dither grain to prevent banding
    float noise = (rand(gl_FragCoord.xy) - 0.5) * (1.5 / 255.0);
    vec3 boosted = clamp(adjust_saturation(color.rgb, saturation) + noise, 0.0, 1.0);
    color.rgb = boosted;

    float coverage = 1.0;
    vec3 frag = vec3(gl_FragCoord.xy, 1.0);
    vec2 local = vec2(dot(clip_row_x, frag), dot(clip_row_y, frag));
    vec2 half_size = shape_size * 0.5;
    vec2 p = local - half_size;

    if (mask_mode == 0) {
        // Rounded rectangle SDF
        float r = corner_radius(p);
        vec2 q = abs(p) - half_size + r;
        float dist = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
        coverage = 1.0 - smoothstep(-0.5, 0.5, dist);
    } else {
        // Alpha mask from client surface texture
        vec2 mask_uv = local / shape_size;
        if (mask_uv.x < 0.0 || mask_uv.x > 1.0 || mask_uv.y < 0.0 || mask_uv.y > 1.0) {
            coverage = 0.0;
        } else {
            float a = texture2D(mask_tex, mask_uv).a;
            coverage = smoothstep(0.2, 0.4, a);
            if (radii != vec4(0.0)) {
                float r = corner_radius(p);
                vec2 q = abs(p) - half_size + r;
                float dist = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
                coverage *= 1.0 - smoothstep(-0.5, 0.5, dist);
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
