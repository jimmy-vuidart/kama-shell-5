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

// Rows of the affine map from gl_FragCoord to pixels of the clip rectangle.
uniform vec3 clip_row_x;
uniform vec3 clip_row_y;
uniform vec2 clip_size;
// Top-left, top-right, bottom-right, bottom-left.
uniform vec4 clip_radius;

float corner_radius(vec2 p) {
    if (p.x < 0.0) {
        return p.y < 0.0 ? clip_radius.x : clip_radius.w;
    }
    return p.y < 0.0 ? clip_radius.y : clip_radius.z;
}

void main() {
    vec4 color = texture2D(tex, v_coords);
#if defined(NO_ALPHA)
    color = vec4(color.rgb, 1.0);
#endif

    vec3 frag = vec3(gl_FragCoord.xy, 1.0);
    vec2 half_size = clip_size * 0.5;
    vec2 p = vec2(dot(clip_row_x, frag), dot(clip_row_y, frag)) - half_size;
    float r = corner_radius(p);
    vec2 q = abs(p) - half_size + r;
    float distance = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;

    color = color * alpha * (1.0 - smoothstep(-0.5, 0.5, distance));

#if defined(DEBUG_FLAGS)
    if (tint == 1.0)
        color = vec4(0.0, 0.2, 0.0, 0.2) + color * 0.8;
#endif

    gl_FragColor = color;
}
