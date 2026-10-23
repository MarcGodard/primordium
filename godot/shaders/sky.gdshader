// Reef sky: day gradient from the sun's height, dusk glow toward the sun, drifting clouds as
// dense as the cosmos says, stars and both moons at night. LIGHT0 is the sun.
shader_type sky;

uniform float cloud = 0.3;
uniform float flare = 0.0;
uniform vec3 moon0_dir = vec3(0.0, 1.0, 0.0);
uniform vec3 moon1_dir = vec3(0.0, 1.0, 0.0);
uniform float moon0_lit = 1.0;
uniform float moon1_lit = 1.0;
uniform float time_days = 0.0;

float hash(vec3 p) { p = fract(p * 0.3183099 + 0.1); p *= 17.0; return fract(p.x * p.y * p.z * (p.x + p.y + p.z)); }
float hash2(vec2 p) { vec3 p3 = fract(vec3(p.xyx) * 0.1031); p3 += dot(p3, p3.yzx + 33.33); return fract((p3.x + p3.y) * p3.z); }
float noise(vec2 p) {
	vec2 i = floor(p); vec2 f = fract(p);
	vec2 u = f * f * (3.0 - 2.0 * f);
	return mix(mix(hash2(i), hash2(i + vec2(1, 0)), u.x), mix(hash2(i + vec2(0, 1)), hash2(i + vec2(1, 1)), u.x), u.y);
}
float fbm(vec2 p) {
	float s = 0.0; float a = 0.5;
	for (int i = 0; i < 5; i++) { s += a * noise(p); p *= 2.03; a *= 0.5; }
	return s;
}

vec3 moon(vec3 d, vec3 dir, float lit, float size) {
	float c = dot(d, normalize(dir));
	float disc = smoothstep(1.0 - size, 1.0 - size * 0.7, c);
	// crude phase: light the side a lit fraction across the disc
	vec3 side = normalize(cross(normalize(dir), vec3(0.0, 1.0, 0.0)) + vec3(0.0001));
	float across = dot(d - normalize(dir) * c, side) / sqrt(size * 2.0);
	float shade = smoothstep(-0.2, 0.2, across + (lit * 2.0 - 1.0));
	return vec3(0.85, 0.86, 0.8) * disc * (0.08 + 0.92 * shade);
}

void sky() {
	vec3 d = EYEDIR;
	vec3 s = LIGHT0_DIRECTION;
	float sh = s.y;
	float day = smoothstep(-0.18, 0.3, sh);
	float dusk = exp(-pow(sh / 0.2, 2.0));
	vec3 zenith = mix(vec3(0.006, 0.01, 0.03), vec3(0.16, 0.34, 0.72), day);
	vec3 horizon = mix(vec3(0.02, 0.03, 0.07), vec3(0.6, 0.74, 0.9), day);
	float toward = pow(max(dot(normalize(d.xz + vec2(0.0001)), normalize(s.xz + vec2(0.0001))), 0.0), 3.0);
	horizon = mix(horizon, vec3(1.0, 0.45, 0.18), dusk * (0.35 + 0.65 * toward));
	float h = max(d.y, 0.0);
	vec3 col = mix(horizon, zenith, pow(h, 0.45));
	// sun: disc and glare
	float cs = max(dot(d, s), 0.0);
	vec3 sun_col = mix(vec3(1.0, 0.55, 0.25), vec3(1.0, 0.96, 0.88), smoothstep(0.0, 0.4, sh));
	sun_col = mix(sun_col, vec3(1.0, 0.7, 1.0), clamp(flare, 0.0, 1.0));
	col += sun_col * (smoothstep(0.9993, 0.9997, cs) * 40.0 + pow(cs, 300.0) * 2.0 + pow(cs, 12.0) * 0.25 * (0.3 + dusk));
	// night: stars, twinkling, hidden by cloud
	float night = 1.0 - smoothstep(-0.25, 0.05, sh);
	vec3 g = floor(d * 420.0);
	float st = hash(g);
	float star = step(0.9975, st) * (0.4 + 0.6 * hash(g + 3.0)) * (0.75 + 0.25 * sin(TIME * 3.0 + st * 100.0));
	col += vec3(0.85, 0.9, 1.0) * star * night * h * (1.0 - cloud) * 2.0;
	// a faint band of the galaxy
	float band = exp(-pow(dot(d, normalize(vec3(0.3, 0.5, 0.8))) / 0.18, 2.0));
	col += vec3(0.25, 0.22, 0.35) * band * fbm(d.xz * 8.0) * night * 0.35 * h;
	col += moon(d, moon0_dir, moon0_lit, 0.0009) * max(night, 0.15) * step(0.0, moon0_dir.y);
	col += moon(d, moon1_dir, moon1_lit, 0.00035) * max(night, 0.15) * step(0.0, moon1_dir.y);
	// clouds: a moving sheet projected on a dome
	if (d.y > 0.0) {
		vec2 uv = d.xz / (d.y + 0.15) * 1.6 + vec2(time_days * 0.8, time_days * 0.3);
		float n = fbm(uv);
		float cov = smoothstep(1.0 - cloud * 1.1 - 0.25, 1.0 - cloud * 1.1 + 0.15, n);
		vec3 lit = mix(vec3(0.06, 0.07, 0.1), vec3(0.95, 0.93, 0.9), day) + vec3(1.0, 0.5, 0.25) * dusk * 0.6 * toward;
		vec3 cc = lit * (0.65 + 0.35 * fbm(uv * 2.5 + 7.0));
		col = mix(col, cc, cov * smoothstep(0.0, 0.12, d.y) * 0.92);
	} else {
		col = mix(horizon * 0.5, vec3(0.01, 0.03, 0.05), clamp(-d.y * 4.0, 0.0, 1.0));
	}
	COLOR = col;
}
