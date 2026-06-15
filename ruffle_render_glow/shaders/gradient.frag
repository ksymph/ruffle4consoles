#version 100

#ifdef GL_FRAGMENT_PRECISION_HIGH
    precision highp float;
#else
    precision mediump float;
#endif

uniform mat4 view_matrix;
uniform mat4 world_matrix;
uniform vec4 mult_color;
uniform vec4 add_color;
uniform mat3 u_matrix;

uniform int u_gradient_type;
uniform int u_repeat_mode;
uniform float u_focal_point;
uniform int u_interpolation;
uniform sampler2D u_gradient_texture;

varying vec2 frag_uv;

vec3 linear_to_srgb(vec3 linear) {
    vec3 a = 12.92 * linear;
    vec3 b = 1.055 * pow(linear, vec3(1.0 / 2.4)) - 0.055;
    vec3 c = step(vec3(0.0031308), linear);
    return mix(a, b, c);
}

void main() {
    float t;
    if (u_gradient_type == 0) {
        t = frag_uv.x;
    } else if (u_gradient_type == 1) {
        t = length(frag_uv * 2.0 - 1.0);
    } else if (u_gradient_type == 2) {
        vec2 uv = frag_uv * 2.0 - 1.0;
        vec2 d = vec2(u_focal_point, 0.0) - uv;
        float l = length(d);
        d /= l;
        t = l / (sqrt(1.0 -  u_focal_point*u_focal_point*d.y*d.y) + u_focal_point*d.x);
    }
    if (u_repeat_mode == 0) {
        t = clamp(t, 0.0, 1.0);
    } else if (u_repeat_mode == 1) {
        t = fract(t);
    } else {
        if (t < 0.0) {
            t = -t;
        }

        if (int(mod(t, 2.0)) == 0) {
            t = fract(t);
        } else {
            t = 1.0 - fract(t);
        }
    }

    vec4 color = texture2D(u_gradient_texture, vec2(t, 0.5));
    color = clamp(mult_color * color + add_color, 0.0, 1.0);

    if (u_interpolation != 0) {
        color = vec4(linear_to_srgb(vec3(color)), color.a);
    }

    float alpha = clamp(color.a, 0.0, 1.0);
    gl_FragColor = vec4(color.rgb * alpha, alpha);
}
