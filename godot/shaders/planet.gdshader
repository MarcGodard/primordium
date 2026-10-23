// The planet: one continent whose west coast runs along longitude 0 (where the coast sites sit),
// open ocean to the west with a pale shelf along the shore, ice past the polar circles.
// The sun lights it; the terminator crosses the coast as the day turns.
shader_type spatial;

varying vec3 objn;

float h3(vec3 p) { p = fract(p * 0.3183099 + 0.1); p *= 17.0; return fract(p.x * p.y * p.z * (p.x + p.y + p.z)); }
float noise3(vec3 x) {
	vec3 i = floor(x); vec3 f = fract(x); f = f * f * (3.0 - 2.0 * f);
	return mix(mix(mix(h3(i), h3(i + vec3(1, 0, 0)), f.x), mix(h3(i + vec3(0, 1, 0)), h3(i + vec3(1, 1, 0)), f.x), f.y),
		mix(mix(h3(i + vec3(0, 0, 1)), h3(i + vec3(1, 0, 1)), f.x), mix(h3(i + vec3(0, 1, 1)), h3(i + vec3(1, 1, 1)), f.x), f.y), f.z);
}
float fbm3(vec3 p) {
	float s = 0.0; float a = 0.5;
	for (int i = 0; i < 5; i++) { s += a * noise3(p); p *= 2.07; a *= 0.5; }
	return s;
}

// longitude of the coastline at a latitude (radians); keep in step with globe.gd
float coast(float lat) { return 0.06 * sin(lat * 9.0) + 0.04 * sin(lat * 23.0 + 1.0) + 0.02; }

void vertex() { objn = NORMAL; }

void fragment() {
	vec3 n = normalize(objn);
	float lat = asin(clamp(n.y, -1.0, 1.0));
	float lon = atan(n.x, n.z);
	float c = coast(lat);
	float detail = fbm3(n * 6.0);
	// continent from the coast eastward ~2.4 rad, with ragged far edges and a few islands
	float east = lon - c;
	float land = smoothstep(-0.004, 0.004, east) * (1.0 - smoothstep(2.1, 2.5, east + (detail - 0.5) * 0.8));
	land = max(land, smoothstep(0.66, 0.7, fbm3(n * 3.0 + 11.0)) * step(east, -0.35));
	vec3 deep = vec3(0.01, 0.04, 0.12);
	vec3 shelf = vec3(0.05, 0.28, 0.36);
	float near = exp(min(east, 0.0) / 0.05);
	vec3 sea = mix(deep, shelf, near * 0.8) * (0.85 + 0.3 * detail);
	vec3 green = mix(vec3(0.12, 0.22, 0.08), vec3(0.32, 0.3, 0.16), fbm3(n * 14.0));
	vec3 dry = vec3(0.52, 0.44, 0.3);
	vec3 ground = mix(green, dry, smoothstep(0.35, 0.7, detail) * (1.0 - abs(lat)));
	vec3 col = mix(sea, ground, land);
	float ice = smoothstep(1.15, 1.25, abs(lat) + (detail - 0.5) * 0.25);
	col = mix(col, vec3(0.9, 0.93, 0.97), ice);
	ALBEDO = pow(col, vec3(2.2));
	ROUGHNESS = mix(0.18, 0.95, max(land, ice));
	SPECULAR = mix(0.7, 0.2, max(land, ice));
}
