// Tide-pool floor. wet 0..1 (smoothed by the view): flooded water vs drying, cracking mud.
shader_type canvas_item;

uniform float wet = 1.0;
uniform float light = 1.0;
uniform vec2 area = vec2(220.0, 140.0);

float hash(vec2 p) { vec3 p3 = fract(vec3(p.xyx) * 0.1031); p3 += dot(p3, p3.yzx + 33.33); return fract((p3.x + p3.y) * p3.z); }
float noise(vec2 p) {
	vec2 i = floor(p); vec2 f = fract(p);
	vec2 u = f * f * (3.0 - 2.0 * f);
	return mix(mix(hash(i), hash(i + vec2(1, 0)), u.x), mix(hash(i + vec2(0, 1)), hash(i + vec2(1, 1)), u.x), u.y);
}
// cell edges of a jittered grid: mud cracks
float cracks(vec2 p) {
	vec2 i = floor(p); vec2 f = fract(p);
	float d1 = 9.0; float d2 = 9.0;
	for (int y = -1; y <= 1; y++) for (int x = -1; x <= 1; x++) {
		vec2 g = vec2(float(x), float(y));
		vec2 o = vec2(hash(i + g), hash(i + g + 17.0));
		float d = length(g + o - f);
		if (d < d1) { d2 = d1; d1 = d; } else if (d < d2) { d2 = d; }
	}
	return smoothstep(0.0, 0.06, d2 - d1);
}

void fragment() {
	vec2 p = UV * area;
	vec3 mud = mix(vec3(0.30, 0.25, 0.18), vec3(0.45, 0.38, 0.27), noise(p * 0.3));
	mud *= mix(0.55, 1.0, cracks(p * 0.12));
	vec3 water = mix(vec3(0.04, 0.16, 0.24), vec3(0.08, 0.30, 0.34), noise(p * 0.05 + TIME * 0.05));
	float caus = pow(noise(p * 0.2 + vec2(TIME * 0.3, TIME * 0.2)) * noise(p * 0.27 - vec2(TIME * 0.25, 0.0)), 3.0);
	water += vec3(0.3, 0.4, 0.4) * caus * light;
	vec3 col = mix(mud, water, wet);
	vec3 night = col * vec3(0.18, 0.22, 0.38);
	COLOR = vec4(mix(night, col, clamp(light, 0.0, 1.0)), 1.0);
}
