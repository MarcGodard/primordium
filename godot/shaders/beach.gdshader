// Tidal beach under the lattice. UV.x = elevation (0 sea floor .. 1 dune).
shader_type canvas_item;

uniform float tide_level = 0.5;
uniform float light = 1.0;
uniform float uv_glow = 0.0;
uniform vec2 grid = vec2(192.0, 128.0);
// elevation range spanned left to right (the cell view shows only the tidal zone)
uniform vec2 elev_range = vec2(0.0, 1.0);

// Hoskins hash: sin-based hashes lose precision at large cell coords and tile visibly
float hash(vec2 p) { vec3 p3 = fract(vec3(p.xyx) * 0.1031); p3 += dot(p3, p3.yzx + 33.33); return fract((p3.x + p3.y) * p3.z); }
float noise(vec2 p) {
	vec2 i = floor(p); vec2 f = fract(p);
	vec2 u = f * f * (3.0 - 2.0 * f);
	return mix(mix(hash(i), hash(i + vec2(1, 0)), u.x), mix(hash(i + vec2(0, 1)), hash(i + vec2(1, 1)), u.x), u.y);
}

void fragment() {
	vec2 cell = UV * grid;
	float elev = mix(elev_range.x, elev_range.y, UV.x);
	float depth = tide_level - elev;
	vec3 sand = mix(vec3(0.42, 0.36, 0.26), vec3(0.62, 0.55, 0.40), noise(cell * 0.35));
	sand *= 0.92 + 0.08 * noise(cell * 2.0);
	// damp band just above the waterline
	sand *= 1.0 - 0.35 * exp(-max(-depth, 0.0) * 30.0);
	vec3 col = sand;
	if (depth > 0.0) {
		vec3 shallow = vec3(0.10, 0.36, 0.40);
		vec3 deep = vec3(0.02, 0.10, 0.22);
		vec3 water = mix(shallow, deep, clamp(depth * 3.0, 0.0, 1.0));
		float caus = noise(cell * 0.25 + vec2(TIME * 0.4, TIME * 0.25)) * noise(cell * 0.31 - vec2(TIME * 0.3, 0.0));
		water += vec3(0.25, 0.35, 0.35) * pow(caus, 3.0) * light;
		col = mix(sand * 0.6, water, clamp(depth * 40.0, 0.0, 0.85));
		float foam = exp(-depth * 120.0) * (0.6 + 0.4 * noise(cell * 0.8 + TIME));
		col = mix(col, vec3(0.9), foam * 0.6);
	}
	vec3 night = col * vec3(0.18, 0.22, 0.38);
	col = mix(night, col, clamp(light, 0.0, 1.0));
	col += vec3(0.35, 0.1, 0.5) * uv_glow;
	COLOR = vec4(col, 1.0);
}
