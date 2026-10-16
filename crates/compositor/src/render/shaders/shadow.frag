// Gaussian soft shadow for rounded rectangles.
// The renderer prepends the version.
precision highp float;

uniform float alpha;
uniform vec2 size;
varying vec2 v_coords;

// The shape that casts the shadow, and where the drawn area sits relative to it.
uniform vec2 shape_size;
uniform vec2 shape_offset;
// Premultiplied shadow color.
uniform vec4 color;
// Top-left, top-right, bottom-right, bottom-left.
uniform vec4 radius;
// Gaussian blur standard deviation in pixels.
uniform float blur;
// Where the shape that casts the shadow is, relative to the shadow: nothing
// is drawn under it, so that a translucent title bar shows what is behind
// the window, not its shadow.
uniform vec2 cutout;

float corner_radius(vec2 p) {
    if (p.x < 0.0) {
        return p.y < 0.0 ? radius.x : radius.w;
    }
    return p.y < 0.0 ? radius.y : radius.z;
}

// Signed distance to a box with rounded corners.
float box(vec2 p, vec2 half_size, float r) {
    vec2 q = abs(p) - half_size + r;
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}

// Polynomial approximation of erf(x) with maximum error < 0.0005.
float erf_approx(float x) {
    float s = sign(x);
    float a = abs(x);
    float x1 = 1.0 + (0.278393 + (0.230389 + 0.078108 * (a * a)) * a) * a;
    x1 *= x1;
    return s * (1.0 - 1.0 / (x1 * x1));
}

void main() {
    vec2 half_size = shape_size * 0.5;
    vec2 p = v_coords * size + shape_offset - half_size;
    float r = corner_radius(p);

    float d = box(p, half_size, r);
    float sigma = max(blur, 0.5);
    // Gaussian error function gives the cumulative distribution for distance d from edge.
    float coverage = 0.5 * (1.0 - erf_approx(d / (sigma * 1.41421356)));
    vec2 caster = p + cutout;
    coverage *= smoothstep(-0.5, 0.5, box(caster, half_size, corner_radius(caster)));

    gl_FragColor = color * coverage * alpha;
}
