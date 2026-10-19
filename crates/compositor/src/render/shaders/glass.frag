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

// Mask mode: 0 = rounded rectangle, 1 = alpha of a client surface,
// 2 = liquid glass: a rounded rectangle that bends and lights what is behind.
uniform int mask_mode;
uniform sampler2D mask_tex;

// Saturation factor (e.g. 1.15)
uniform float saturation;

// Liquid glass, in pixels of the target: the width of the curved rim, how
// far it shifts what is seen through it at most, how bright its specular
// edge is, and a light lift of the whole pane.
uniform vec4 liquid;

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

vec4 blurred(vec2 frag) {
    vec2 uv = clamp((frag - blur_origin) * blur_texel, vec2(0.0), blur_max);
    vec4 color = texture2D(tex, uv);
#if defined(NO_ALPHA)
    color = vec4(color.rgb, 1.0);
#endif
    return color;
}

// Signed distance to the rounded rectangle, negative inside.
float rounded_distance(vec2 p, vec2 half_size) {
    float r = corner_radius(p);
    vec2 q = abs(p) - half_size + r;
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}

// The slope of a convex squircle rim, `1 - (1 - x)^4` under a fourth root,
// at `x` from its outer edge (0) to the flat pane (1): smooth where it meets
// the pane, so what is seen through it bends without a crease.
float rim_height(float x) {
    float u = 1.0 - x;
    return pow(max(1.0 - u * u * u * u, 0.0), 0.25);
}

float rim_slope(float x) {
    float u = 1.0 - x;
    float rest = max(1.0 - u * u * u * u, 1e-3);
    return min(u * u * u * pow(rest, -0.75), 12.0);
}

// Liquid glass: what is behind is seen through a pane whose rim curves down
// to its edge, refracted as by glass of index 1.5, lit from the top left.
vec4 liquid_glass(vec2 p, vec2 half_size) {
    float bezel = max(liquid.x, 1.0);
    float d = rounded_distance(p, half_size);
    float x = clamp(-d / bezel, 0.0, 1.0);

    // The outward normal of the edge, from the gradient of the distance.
    vec2 e = vec2(0.5, 0.0);
    vec2 normal = vec2(
        rounded_distance(p + e.xy, half_size) - rounded_distance(p - e.xy, half_size),
        rounded_distance(p + e.yx, half_size) - rounded_distance(p - e.yx, half_size));
    normal = length(normal) > 1e-4 ? normalize(normal) : vec2(0.0);

    // Snell: a ray straight down meets the rim tilted by `tilt`, and leaves
    // it bent by `tilt - refracted`; how far it lands from where it would
    // have depends on the thickness of the glass there.
    float tilt = atan(rim_slope(x));
    float refracted = asin(sin(tilt) / 1.5);
    float shift = tan(tilt - refracted) * rim_height(x);
    // In the frame of the pane, towards its middle: the rim magnifies.
    vec2 offset_local = -normal * shift * liquid.y;

    // Back to pixels of the target: the inverse of the linear part of the
    // map from target pixels to the pane.
    mat2 to_local = mat2(clip_row_x.x, clip_row_y.x, clip_row_x.y, clip_row_y.y);
    float det = to_local[0][0] * to_local[1][1] - to_local[1][0] * to_local[0][1];
    mat2 to_frag = mat2(to_local[1][1], -to_local[0][1], -to_local[1][0], to_local[0][0])
        / (abs(det) > 1e-6 ? det : 1.0);
    vec2 offset = to_frag * offset_local;

    // A slight dispersion: red bends less than blue.
    vec4 color = blurred(gl_FragCoord.xy + offset);
    color.r = blurred(gl_FragCoord.xy + offset * 0.94).r;
    color.b = blurred(gl_FragCoord.xy + offset * 1.06).b;

    color.rgb = adjust_saturation(color.rgb, saturation);
    color.rgb = mix(color.rgb, vec3(1.0), liquid.w);

    // The specular edge: a line of light along the very edge, and a softer
    // glow down the rim, both strongest where the edge faces the light (top
    // left) or its reflection (bottom right).
    vec2 light = normalize(vec2(-0.55, -0.85));
    float facing = abs(dot(normal, light));
    float width = max(1.5, bezel * 0.07);
    float line = 1.0 - smoothstep(width * 0.5, width, -d);
    float glow = pow(1.0 - x, 3.0);
    float specular = liquid.z * (line * (0.5 + 0.5 * facing) + glow * 0.5 * facing * facing);
    color.rgb = mix(color.rgb, vec3(1.0), clamp(specular, 0.0, 1.0));
    return color;
}

void main() {
    vec3 frag = vec3(gl_FragCoord.xy, 1.0);
    vec2 local = vec2(dot(clip_row_x, frag), dot(clip_row_y, frag));
    vec2 half_size = shape_size * 0.5;
    vec2 p = local - half_size;

    vec4 color;
    if (mask_mode == 2) {
        color = liquid_glass(p, half_size);
    } else {
        color = blurred(gl_FragCoord.xy);
        color.rgb = adjust_saturation(color.rgb, saturation);
    }
    // A subtle grain against banding.
    float noise = (rand(gl_FragCoord.xy) - 0.5) * (1.5 / 255.0);
    color.rgb = clamp(color.rgb + noise, 0.0, 1.0);

    float coverage = 0.0;
    if (mask_mode == 0 || mask_mode == 2) {
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
