// Dual Kawase downsampling pass.
precision highp float;

uniform sampler2D tex;
uniform vec2 halfpixel;
uniform float offset;
varying vec2 v_uv;

void main() {
    vec2 d = halfpixel * offset;
    vec4 sum = texture2D(tex, v_uv) * 4.0;
    sum += texture2D(tex, v_uv - d);
    sum += texture2D(tex, v_uv + d);
    sum += texture2D(tex, v_uv + vec2(d.x, -d.y));
    sum += texture2D(tex, v_uv - vec2(d.x, -d.y));
    gl_FragColor = sum * 0.125;
}
