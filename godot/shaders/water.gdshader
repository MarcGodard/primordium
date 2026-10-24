// Sea surface at the tide level: swell that grows with storms, colour by water thickness behind
// it (depth texture), foam where it is thin, a sheen from the sun. Seen from below too.
shader_type spatial;
render_mode blend_mix, cull_disabled, depth_draw_always, specular_schlick_ggx;

uniform sampler2D depth_tex : hint_depth_texture, filter_linear_mipmap;
uniform vec3 shallow : source_color = vec3(0.12, 0.5, 0.52);
uniform vec3 deep : source_color = vec3(0.01, 0.07, 0.16);
uniform float swell = 0.4;
uniform float daylight = 1.0;
// far from here the mesh is coarse: vertex swell fades out before it would alias
uniform vec2 calm_center = vec2(0.0);

varying vec3 wpos;

float h2(vec2 p) { vec3 p3 = fract(vec3(p.xyx) * 0.1031); p3 += dot(p3, p3.yzx + 33.33); return fract((p3.x + p3.y) * p3.z); }
float noise(vec2 p) {
	vec2 i = floor(p); vec2 f = fract(p);
	vec2 u = f * f * (3.0 - 2.0 * f);
	return mix(mix(h2(i), h2(i + vec2(1, 0)), u.x), mix(h2(i + vec2(0, 1)), h2(i + vec2(1, 1)), u.x), u.y);
}

float waves(vec2 p, float t) {
	float w = sin(p.x * 0.09 + p.y * 0.05 + t * 1.1) * 0.6;
	w += sin(p.x * -0.05 + p.y * 0.13 + t * 1.6) * 0.35;
	w += sin(p.x * 0.21 + p.y * 0.17 + t * 2.3) * 0.15;
	return w * swell;
}

void vertex() {
	vec3 w = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
	VERTEX.y += waves(w.xz, TIME) * (1.0 - smoothstep(500.0, 700.0, distance(w.xz, calm_center)));
	wpos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
}

void fragment() {
	float e = 0.5;
	float t = TIME;
	vec2 p = wpos.xz;
	float hx = waves(p + vec2(e, 0.0), t) - waves(p - vec2(e, 0.0), t);
	float hz = waves(p + vec2(0.0, e), t) - waves(p - vec2(0.0, e), t);
	vec2 ripple = vec2(noise(p * 0.12 + t * 0.4) - 0.5, noise(p * 0.12 - t * 0.35 + 9.0) - 0.5) * 0.18;
	vec3 n = normalize(vec3(-hx / (2.0 * e) + ripple.x, 1.0, -hz / (2.0 * e) + ripple.y));
	NORMAL = (VIEW_MATRIX * vec4(n, 0.0)).xyz;
	float depth = textureLod(depth_tex, SCREEN_UV, 0.0).r;
	vec4 v = INV_PROJECTION_MATRIX * vec4(SCREEN_UV * 2.0 - 1.0, depth, 1.0);
	v.xyz /= v.w;
	float thick = max(-v.z - (-VERTEX.z), 0.0);
	float absorb = 1.0 - exp(-thick * 0.02);
	vec3 col = mix(shallow, deep, absorb);
	float foam = (1.0 - smoothstep(0.0, 1.6, thick)) * (0.55 + 0.45 * noise(p * 0.8 + t));
	foam += smoothstep(0.55, 0.95, noise(p * 0.15 + t * 0.2)) * swell * 0.25;
	col = mix(col, vec3(0.92, 0.96, 0.97), clamp(foam, 0.0, 1.0) * 0.8);
	ALBEDO = pow(col, vec3(2.2)) * mix(0.35, 1.0, daylight);
	float fres = pow(1.0 - clamp(dot(normalize(NORMAL), normalize(VIEW)), 0.0, 1.0), 4.0);
	ALPHA = clamp(0.12 + absorb * 0.8 + fres * 0.4 + foam * 0.5, 0.0, 0.95);
	ROUGHNESS = 0.12;
	SPECULAR = 0.7;
	METALLIC = 0.0;
}
