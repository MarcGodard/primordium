// Rocky shore: sand on the flats, rock on the slopes, darker and slimed with a green film where
// the tide has been, caustics on whatever lies under water.
shader_type spatial;

uniform float water_level = 40.0;
uniform float daylight = 1.0;

varying vec3 wpos;
varying vec3 wnorm;

float h2(vec2 p) { vec3 p3 = fract(vec3(p.xyx) * 0.1031); p3 += dot(p3, p3.yzx + 33.33); return fract((p3.x + p3.y) * p3.z); }
float noise(vec2 p) {
	vec2 i = floor(p); vec2 f = fract(p);
	vec2 u = f * f * (3.0 - 2.0 * f);
	return mix(mix(h2(i), h2(i + vec2(1, 0)), u.x), mix(h2(i + vec2(0, 1)), h2(i + vec2(1, 1)), u.x), u.y);
}
float fbm(vec2 p) {
	float s = 0.0; float a = 0.5;
	for (int i = 0; i < 4; i++) { s += a * noise(p); p *= 2.1; a *= 0.5; }
	return s;
}

void vertex() {
	wpos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
	wnorm = normalize((MODEL_MATRIX * vec4(NORMAL, 0.0)).xyz);
}

void fragment() {
	float slope = 1.0 - wnorm.y;
	float n = fbm(wpos.xz * 0.12);
	vec3 sand = mix(vec3(0.55, 0.48, 0.34), vec3(0.72, 0.64, 0.47), n);
	vec3 rock = mix(vec3(0.24, 0.22, 0.21), vec3(0.42, 0.39, 0.35), fbm(wpos.xz * 0.3 + wpos.y * 0.2));
	vec3 col = mix(sand, rock, smoothstep(0.12, 0.35, slope + (n - 0.5) * 0.3));
	float below = water_level - wpos.y;
	// wet band and tide film: darker, greener, under and just above the water
	float wet = smoothstep(-4.0, 1.0, below);
	col = mix(col, col * vec3(0.55, 0.62, 0.55) + vec3(0.0, 0.03, 0.01) * fbm(wpos.xz * 0.5), wet);
	if (below > 0.0) {
		vec2 c = wpos.xz * 0.22;
		float caus = noise(c + vec2(TIME * 0.35, TIME * 0.2)) * noise(c * 1.3 - vec2(TIME * 0.25, -TIME * 0.1));
		col += vec3(0.5, 0.75, 0.7) * pow(caus, 2.5) * 1.6 * daylight * exp(-below * 0.04);
		col *= mix(vec3(1.0), vec3(0.45, 0.75, 0.8), clamp(below * 0.03, 0.0, 0.8));
	}
	ALBEDO = pow(col, vec3(2.2));
	ROUGHNESS = mix(0.95, 0.3, wet);
	SPECULAR = mix(0.2, 0.6, wet);
}
