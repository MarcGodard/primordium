// A cell: a translucent membrane (bright at the rim, see-through in the middle) tinted by what
// its genes do; instance custom.x = 1 when selected, custom.y = 1 for the inner core.
shader_type spatial;
render_mode blend_mix, depth_draw_always, cull_back, specular_schlick_ggx;

varying vec4 tint;
varying vec4 extra;

void vertex() {
	tint = COLOR;
	extra = INSTANCE_CUSTOM;
	vec3 o = MODEL_MATRIX[3].xyz;
	float phase = fract(sin(dot(o.xz, vec2(12.9898, 78.233))) * 43758.5453) * 6.283;
	VERTEX *= 1.0 + 0.04 * sin(TIME * 1.3 + phase) + 0.02 * sin(VERTEX.y * 6.0 + TIME * 2.0 + phase);
}

void fragment() {
	vec3 c = pow(tint.rgb, vec3(2.2));
	float fres = pow(1.0 - clamp(dot(NORMAL, VIEW), 0.0, 1.0), 2.2);
	if (extra.y > 0.5) {
		// genome core: dense, darker, glowing faintly
		ALBEDO = c * 0.35;
		EMISSION = c * 0.25;
		ALPHA = 0.9;
	} else {
		ALBEDO = c;
		ALPHA = clamp(0.18 + fres * 0.8, 0.0, 0.95);
		EMISSION = c * fres * 0.35;
		SSS_STRENGTH = 0.5;
	}
	if (extra.x > 0.5) {
		EMISSION += vec3(1.0, 0.95, 0.8) * (0.4 + 0.3 * sin(TIME * 4.0)) * (0.3 + fres);
	}
	ROUGHNESS = 0.2;
	SPECULAR = 0.6;
}
