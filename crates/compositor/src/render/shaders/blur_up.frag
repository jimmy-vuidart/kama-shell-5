// Dual Kawase upsampling pass.
precision highp float;

uniform sampler2D tex;
uniform vec2 halfpixel;
uniform float offset;
varying vec2 v_uv;

void main() {
    vec2 d = halfpixel * offset;
    vec4 sum = vec4(0.0);
    sum += texture2D(tex, v_uv + vec2(-d.x * 2.0, 0.0));
    sum += texture2D(tex, v_uv + vec2(-d.x, d.y)) * 2.0;
    sum += texture2D(tex, v_uv + vec2(0.0, d.y * 2.0));
    sum += texture2D(tex, v_uv + vec2(d.x, d.y)) * 2.0;
    sum += texture2D(tex, v_uv + vec2(d.x * 2.0, 0.0));
    sum += texture2D(tex, v_uv + vec2(d.x, -d.y)) * 2.0;
    sum += texture2D(tex, v_uv + vec2(0.0, -d.y * 2.0));
    sum += texture2D(tex, v_uv + vec2(-d.x, -d.y)) * 2.0;
    gl_FragColor = sum / 12.0;
}
