// Cloud shell: cover follows the cosmos cloud fraction and drifts with time.
shader_type spatial;
render_mode blend_mix, depth_draw_never, cull_back;

uniform float cover = 0.4;
uniform float t = 0.0;
varying vec3 objn;

float h3(vec3 p) { p = fract(p * 0.3183099 + 0.1); p *= 17.0; return fract(p.x * p.y * p.z * (p.x + p.y + p.z)); }
float noise3(vec3 x) {
	vec3 i = floor(x); vec3 f = fract(x); f = f * f * (3.0 - 2.0 * f);
	return mix(mix(mix(h3(i), h3(i + vec3(1, 0, 0)), f.x), mix(h3(i + vec3(0, 1, 0)), h3(i + vec3(1, 1, 0)), f.x), f.y),
		mix(mix(h3(i + vec3(0, 0, 1)), h3(i + vec3(1, 0, 1)), f.x), mix(h3(i + vec3(0, 1, 1)), h3(i + vec3(1, 1, 1)), f.x), f.y), f.z);
}

void vertex() { objn = NORMAL; }

void fragment() {
	vec3 n = normalize(objn);
	vec3 p = n * 4.0 + vec3(t * 0.6, 0.0, t * 0.25);
	float f = 0.0; float a = 0.5;
	for (int i = 0; i < 5; i++) { f += a * noise3(p); p *= 2.1; a *= 0.5; }
	float c = smoothstep(1.0 - cover * 1.1 - 0.12, 1.0 - cover * 1.1 + 0.12, f);
	ALBEDO = vec3(1.0);
	ALPHA = c * 0.85;
	ROUGHNESS = 1.0;
}
