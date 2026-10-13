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
// 0 paints the shape in `color`. 1 paints it with a ramp instead, from the
// top of the shape to its bottom: eight colors (premultiplied), each with
// how far down it sits, from 0 to 1 and never going back up.
uniform float ramp;
uniform vec4 stop_0;
uniform vec4 stop_1;
uniform vec4 stop_2;
uniform vec4 stop_3;
uniform vec4 stop_4;
uniform vec4 stop_5;
uniform vec4 stop_6;
uniform vec4 stop_7;
// Where the first four colors sit, then the last four.
uniform vec4 stops_upper;
uniform vec4 stops_lower;
// Top-left, top-right, bottom-right, bottom-left.
uniform vec4 radius;
// 0 fills the shape; otherwise the thickness of the ring.
uniform float border;

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

// How far `t` is from `from` to `to`, from 0 to 1. Two colors at nearly the
// same place make a sharp break.
float along(float from, float to, float t) {
    return clamp((t - from) / max(to - from, 0.00001), 0.0, 1.0);
}

// The color of the ramp at `t`, from 0 at the top to 1 at the bottom.
vec4 ramp_color(float t) {
    vec4 result = stop_0;
    result = mix(result, stop_1, along(stops_upper.x, stops_upper.y, t));
    result = mix(result, stop_2, along(stops_upper.y, stops_upper.z, t));
    result = mix(result, stop_3, along(stops_upper.z, stops_upper.w, t));
    result = mix(result, stop_4, along(stops_upper.w, stops_lower.x, t));
    result = mix(result, stop_5, along(stops_lower.x, stops_lower.y, t));
    result = mix(result, stop_6, along(stops_lower.y, stops_lower.z, t));
    result = mix(result, stop_7, along(stops_lower.z, stops_lower.w, t));
    return result;
}

void main() {
    vec2 half_size = shape_size * 0.5;
    vec2 p = v_coords * size + shape_offset - half_size;
    float r = corner_radius(p);

    float coverage = 1.0 - smoothstep(-0.5, 0.5, box(p, half_size, r));
    if (border > 0.0) {
        float inner = box(p, half_size - border, max(r - border, 0.0));
        coverage *= smoothstep(-0.5, 0.5, inner);
    }

    vec4 paint = color;
    if (ramp > 0.5) {
        paint = ramp_color((p.y + half_size.y) / shape_size.y);
    }
    vec4 result = paint * coverage * alpha;

#if defined(DEBUG_FLAGS)
    if (tint == 1.0)
        result = vec4(0.0, 0.2, 0.0, 0.2) + result * 0.8;
#endif

    gl_FragColor = result;
}
