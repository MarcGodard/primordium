// Gold marker over a creature: a camera-facing downward chevron (the instance sets its place
// above the body and a scale that keeps it one size on screen), slow bob and pulse. Depth
// tested: bodies in front hide it.
shader_type spatial;
render_mode unshaded, blend_add, depth_draw_never, cull_disabled;

uniform vec4 tint : source_color = vec4(1.0, 0.8, 0.3, 1.0);

void vertex() {
	// billboard: keep the instance's position and scale, face the camera
	float s = length(MODEL_MATRIX[0].xyz);
	vec4 at = MODEL_MATRIX[3] + vec4(0.0, s * 0.15 * sin(TIME * 2.5), 0.0, 0.0);
	MODELVIEW_MATRIX = VIEW_MATRIX * mat4(INV_VIEW_MATRIX[0], INV_VIEW_MATRIX[1], INV_VIEW_MATRIX[2], at);
	MODELVIEW_MATRIX = MODELVIEW_MATRIX * mat4(vec4(s, 0, 0, 0), vec4(0, s, 0, 0), vec4(0, 0, s, 0), vec4(0, 0, 0, 1));
}

void fragment() {
	// a "V": two thick strokes meeting at the bottom centre
	vec2 p = UV - vec2(0.5, 0.15);
	p.y = 1.0 - UV.y - 0.15;
	float x = abs(UV.x - 0.5);
	float y = (1.0 - UV.y) - 0.15;
	float d = abs(y - x * 1.4) / 1.72;
	float inside = step(0.0, y) * step(y, 0.75);
	float stroke = (1.0 - smoothstep(0.06, 0.1, d)) * inside;
	float pulse = 0.75 + 0.25 * sin(TIME * 3.0);
	ALBEDO = tint.rgb * 2.0;
	ALPHA = stroke * pulse;
}
