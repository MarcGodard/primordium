// A body cell. Instance colour = expressed polymerase, metabolism, membrane, pigment shade;
// custom = adhesion, light reaching it, selected (1) / adrift (0.5), height in the body + 64 x lineage.
// Tint = what the cell's genes are doing there, as on scales 3 and 4, plus gold for glue.
// Cells breathe, and sway more the higher they stand in the swell.
shader_type spatial;
render_mode diffuse_burley, specular_schlick_ggx;

uniform float sway = 0.3;
// reef-wide mean expression (polymerase, metabolism, membrane, shade) and adhesion: a cell is
// tinted by what it does more than the typical cell, so jobs taken by position stand out
uniform vec4 mean_expr = vec4(0.35, 0.4, 0.45, 0.38);
uniform float mean_adh = 0.35;
// 0: tint by job; 1: tint by lineage (which founder a body descends from)
uniform float by_lineage = 0.0;
uniform float daylight = 1.0;

varying vec4 expr;
varying vec4 extra;

void vertex() {
	expr = COLOR;
	extra = INSTANCE_CUSTOM;
	vec3 origin = MODEL_MATRIX[3].xyz;
	float phase = fract(sin(dot(origin.xz, vec2(12.9898, 78.233))) * 43758.5453) * 6.283;
	VERTEX *= 1.0 + 0.05 * sin(TIME * 1.7 + phase);
	float z = mod(extra.w, 64.0);
	vec2 drift = vec2(sin(TIME * 0.9 + origin.x * 0.05), cos(TIME * 0.7 + origin.z * 0.05)) * sway * z * z * 0.02;
	VERTEX.xz += drift / max(length(MODEL_MATRIX[0].xyz), 0.01);
}

void fragment() {
	vec3 c_poly = vec3(0.25, 0.8, 0.85);
	vec3 c_synth = vec3(0.35, 0.8, 0.3);
	vec3 c_amph = vec3(0.95, 0.88, 0.72);
	vec3 c_pig = vec3(0.75, 0.22, 0.32);
	vec3 c_adh = vec3(1.0, 0.75, 0.2);
	vec4 rel = expr / max(mean_expr, vec4(0.02));
	float ra = extra.x / max(mean_adh, 0.02);
	float w0 = pow(max(rel.x - 0.8, 0.0), 2.0);
	float w1 = pow(max(rel.y - 0.8, 0.0), 2.0);
	float w2 = pow(max(rel.z - 0.8, 0.0), 2.0);
	float w3 = pow(max(rel.w - 0.8, 0.0), 2.0);
	float w4 = pow(max(ra - 0.8, 0.0), 2.0);
	float tot = w0 + w1 + w2 + w3 + w4;
	vec3 plain = vec3(0.7, 0.68, 0.62);
	vec3 col = tot > 0.0 ? (c_poly * w0 + c_synth * w1 + c_amph * w2 + c_pig * w3 + c_adh * w4) / tot : plain;
	col = mix(plain, col, clamp(tot * 6.0, 0.0, 1.0));
	if (by_lineage > 0.5) {
		// golden-ratio hues per founder: neighbouring founder numbers get distant colours
		float h = fract(floor(extra.w / 64.0) * 0.61803);
		col = clamp(abs(mod(h * 6.0 + vec3(0.0, 4.0, 2.0), 6.0) - 3.0) - 1.0, 0.0, 1.0);
		col = mix(vec3(0.85), col, 0.75);
	}
	col = pow(col, vec3(2.2));
	ALBEDO = col;
	ROUGHNESS = 0.32;
	SPECULAR = 0.55;
	SSS_STRENGTH = 0.7;
	BACKLIGHT = col * 0.45;
	RIM = 0.35;
	RIM_TINT = 0.6;
	float fres = pow(1.0 - clamp(dot(NORMAL, VIEW), 0.0, 1.0), 3.0);
	vec3 glow = col * fres * 0.15;
	if (extra.z > 0.75) {
		glow += vec3(1.0, 0.95, 0.8) * (0.35 + 0.25 * sin(TIME * 4.0)) * (0.3 + fres);
	} else if (extra.z > 0.25) {
		glow += vec3(0.5, 0.9, 1.0) * (0.6 + 0.4 * sin(TIME * 6.0 + mod(extra.w, 64.0)));
	}
	EMISSION = glow * mix(1.0, 1.8, 1.0 - daylight);
}
