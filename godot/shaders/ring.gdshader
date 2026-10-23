// Gold marker ring over a creature: camera-facing quad, soft ring, slow pulse.
shader_type spatial;
render_mode unshaded, blend_add, depth_draw_never, cull_disabled;

uniform vec4 tint : source_color = vec4(1.0, 0.8, 0.3, 1.0);

void vertex() {
	// billboard: keep the instance's position and scale, face the camera
	float s = length(MODEL_MATRIX[0].xyz);
	MODELVIEW_MATRIX = VIEW_MATRIX * mat4(INV_VIEW_MATRIX[0], INV_VIEW_MATRIX[1], INV_VIEW_MATRIX[2], MODEL_MATRIX[3]);
	MODELVIEW_MATRIX = MODELVIEW_MATRIX * mat4(vec4(s, 0, 0, 0), vec4(0, s, 0, 0), vec4(0, 0, s, 0), vec4(0, 0, 0, 1));
}

void fragment() {
	float r = length(UV - 0.5) * 2.0;
	float pulse = 0.75 + 0.25 * sin(TIME * 3.0);
	float ring = smoothstep(0.78, 0.86, r) * (1.0 - smoothstep(0.9, 0.98, r));
	ALBEDO = tint.rgb * 2.0;
	ALPHA = ring * pulse;
}
