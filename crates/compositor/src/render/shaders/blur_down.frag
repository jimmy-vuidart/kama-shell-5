// Dual Kawase downsampling pass.
precision highp float;

uniform sampler2D tex;
uniform vec2 halfpixel;
uniform float offset;
// The input texture is larger than what it holds: `uv_scale` maps the pass's
// [0, 1] onto the part in use, `uv_max` keeps samples inside it.
uniform vec2 uv_scale;
uniform vec2 uv_max;
varying vec2 v_uv;

vec4 at(vec2 uv) {
    return texture2D(tex, clamp(uv, halfpixel, uv_max));
}

void main() {
    vec2 uv = v_uv * uv_scale;
    vec2 d = halfpixel * offset;
    vec4 sum = at(uv) * 4.0;
    sum += at(uv - d);
    sum += at(uv + d);
    sum += at(uv + vec2(d.x, -d.y));
    sum += at(uv - vec2(d.x, -d.y));
    gl_FragColor = sum * 0.125;
}
