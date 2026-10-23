// Deep space for the hub: stars of a few brightnesses, a faint nebula and a galaxy band.
shader_type sky;

float hash(vec3 p) { p = fract(p * 0.3183099 + 0.1); p *= 17.0; return fract(p.x * p.y * p.z * (p.x + p.y + p.z)); }
float noise3(vec3 x) {
	vec3 i = floor(x); vec3 f = fract(x); f = f * f * (3.0 - 2.0 * f);
	return mix(mix(mix(hash(i), hash(i + vec3(1, 0, 0)), f.x), mix(hash(i + vec3(0, 1, 0)), hash(i + vec3(1, 1, 0)), f.x), f.y),
		mix(mix(hash(i + vec3(0, 0, 1)), hash(i + vec3(1, 0, 1)), f.x), mix(hash(i + vec3(0, 1, 1)), hash(i + vec3(1, 1, 1)), f.x), f.y), f.z);
}
float fbm(vec3 p) {
	float s = 0.0; float a = 0.5;
	for (int i = 0; i < 5; i++) { s += a * noise3(p); p *= 2.03; a *= 0.5; }
	return s;
}

void sky() {
	vec3 d = EYEDIR;
	vec3 col = vec3(0.004, 0.006, 0.015);
	float neb = fbm(d * 2.5 + 3.0);
	col += vec3(0.12, 0.05, 0.2) * pow(neb, 3.0) * 1.4 + vec3(0.02, 0.08, 0.14) * pow(fbm(d * 3.5 - 7.0), 3.0) * 1.6;
	float band = exp(-pow(dot(d, normalize(vec3(0.2, 0.9, 0.4))) / 0.22, 2.0));
	col += vec3(0.18, 0.16, 0.24) * band * fbm(d * 9.0) * 0.6;
	for (int k = 0; k < 3; k++) {
		float sc = 320.0 + float(k) * 260.0;
		vec3 g = floor(d * sc);
		float h = hash(g + float(k) * 13.0);
		float star = step(0.9965 + float(k) * 0.001, h);
		vec3 tint = mix(vec3(0.7, 0.8, 1.0), vec3(1.0, 0.85, 0.7), hash(g + 5.0));
		col += tint * star * (0.6 + 0.4 * sin(TIME * (1.0 + h * 3.0) + h * 40.0)) * (1.1 - float(k) * 0.3);
	}
	COLOR = col;
}
