// A rounded rectangle, filled or as a ring, or just a part of one.
// The renderer prepends the version.
precision highp float;

uniform float alpha;
uniform vec2 size;
varying vec2 v_coords;

#if defined(DEBUG_FLAGS)
uniform float tint;
#endif

// The rounded rectangle, and where the drawn area sits inside it.
uniform vec2 shape_size;
uniform vec2 shape_offset;
// Premultiplied.
uniform vec4 color;
// Top-left, top-right, bottom-right, bottom-left.
uniform vec4 radius;
// 0 fills the shape; otherwise the thickness of the ring.
uniform float border;
// One pixel of the screen, in the units of the shape: edges are smoothed over
// a pixel, whatever the scale.
uniform float pixel;

float corner_radius(vec2 p) {
    if (p.x < 0.0) {
        return p.y < 0.0 ? radius.x : radius.w;
    }
    return p.y < 0.0 ? radius.y : radius.z;
}

// Signed distance to a box of half size `half_size` with corners of radius `r`.
float box(vec2 p, vec2 half_size, float r) {
    vec2 q = abs(p) - half_size + r;
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}

void main() {
    vec2 half_size = shape_size * 0.5;
    vec2 p = v_coords * size + shape_offset - half_size;
    float r = corner_radius(p);

    float edge = 0.5 * pixel;
    float coverage = 1.0 - smoothstep(-edge, edge, box(p, half_size, r));
    if (border > 0.0) {
        float inner = box(p, half_size - border, max(r - border, 0.0));
        coverage *= smoothstep(-edge, edge, inner);
    }

    vec4 result = color * coverage * alpha;

#if defined(DEBUG_FLAGS)
    if (tint == 1.0)
        result = vec4(0.0, 0.2, 0.0, 0.2) + result * 0.8;
#endif

    gl_FragColor = result;
}
