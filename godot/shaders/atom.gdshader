// One quad per atom. Zoomed out: shaded disc coloured by element.
// Zoomed in (px_per_cell high): nucleus, shell rings, orbiting electrons.
// Shared (bonded) outer electrons draw white; an excited atom's electron
// sits on a wide glowing orbit. All derived from z, so a new element needs no art.
shader_type canvas_item;
render_mode blend_mix;

uniform float px_per_cell = 8.0;
uniform vec4 palette[9];
uniform float light = 1.0;

varying flat vec4 custom;

void vertex() {
	custom = INSTANCE_CUSTOM;
}

float dot_at(vec2 p, vec2 c, float r, float aa) {
	return 1.0 - smoothstep(r - aa, r + aa, length(p - c));
}

float ring(float r, float at, float w, float aa) {
	return 1.0 - smoothstep(w - aa, w + aa, abs(r - at));
}

void fragment() {
	vec2 p = (UV - 0.5) * 2.0;
	float r = length(p);
	int z = int(custom.x + 0.5);
	// +10 on the bond count marks a frozen atom in a surrogate band
	bool frozen = custom.y > 9.5;
	float nb = frozen ? custom.y - 10.0 : custom.y;
	float exc = custom.z;
	float phase = custom.w * 6.2831;
	vec3 col = palette[z].rgb;
	float aa = 2.5 / max(px_per_cell, 1.0);
	float detail = smoothstep(18.0, 48.0, px_per_cell);
	float t = TIME;

	// far view: disc
	float dr = 0.78 + 0.06 * nb;
	float disc = 1.0 - smoothstep(dr - aa, dr + aa, r);
	vec3 shade = col * (0.7 + 0.45 * (1.0 - r / dr)) * max(light, 0.45);
	vec4 far = vec4(shade, disc);

	// near view: atom anatomy
	int inner = min(z, 2);
	int outer = max(z - 2, 0);
	float glow = exp(-r * 3.0) * 0.35;
	vec3 c = col * glow;
	float a = glow;
	float nuc = dot_at(p, vec2(0.0), 0.10 + 0.01 * float(z), aa);
	c = mix(c, mix(col, vec3(1.0, 0.9, 0.8), 0.35) * 1.3, nuc);
	a = max(a, nuc);
	float r1 = 0.36;
	float r2 = 0.72;
	float rr = ring(r, r1, 0.008, aa) * 0.5;
	if (outer > 0) rr = max(rr, ring(r, r2, 0.008, aa) * 0.5);
	c = mix(c, col * 1.2, rr);
	a = max(a, rr);
	for (int k = 0; k < 2; k++) {
		if (k >= inner) break;
		float ang = phase + t * 2.3 + 3.14159 * float(k);
		float d = dot_at(p, r1 * vec2(cos(ang), sin(ang)), 0.05, aa);
		c = mix(c, vec3(0.6, 0.85, 1.0), d);
		a = max(a, d);
	}
	for (int k = 0; k < 6; k++) {
		if (k >= outer) break;
		bool shared = float(k) < nb;
		bool excited = exc > 0.0 && k == outer - 1 && !shared;
		float rad = excited ? 0.93 : r2;
		float ang = phase * 1.7 - t * 1.1 + 6.2831 * float(k) / float(outer);
		float d = dot_at(p, rad * vec2(cos(ang), sin(ang)), excited ? 0.08 : 0.055, aa);
		vec3 ec = shared ? vec3(1.0) : (excited ? vec3(1.0, 0.9, 0.3) : vec3(0.55, 0.8, 1.0));
		c = mix(c, ec, d);
		a = max(a, d);
	}
	vec4 near = vec4(c * max(light, 0.6), a);

	vec4 o = mix(far, near, detail);
	// excitation halo, both views
	float halo = exc > 0.0 ? (1.0 - smoothstep(0.55, 1.0, r)) * 0.55 * (0.7 + 0.3 * sin(t * 9.0 + phase)) : 0.0;
	o.rgb = mix(o.rgb, vec3(1.0, 0.92, 0.35), halo * (1.0 - o.a * 0.5));
	o.a = max(o.a, halo);
	if (frozen) {
		o.rgb = mix(o.rgb, vec3(dot(o.rgb, vec3(0.33))), 0.8) * 0.5;
		o.a *= 0.25;
	}
	COLOR = o;
}
