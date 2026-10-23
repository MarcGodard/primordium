// Thin blue rim of air, brightest on the day side.
shader_type spatial;
render_mode unshaded, blend_add, depth_draw_never, cull_back;

uniform vec3 sun_dir = vec3(0.0, 0.0, 1.0);
varying vec3 wn;

void vertex() { wn = normalize((MODEL_MATRIX * vec4(NORMAL, 0.0)).xyz); }

void fragment() {
	float rim = pow(1.0 - clamp(dot(NORMAL, VIEW), 0.0, 1.0), 2.5);
	float lit = 0.12 + 0.88 * smoothstep(-0.25, 0.45, dot(wn, normalize(sun_dir)));
	ALBEDO = vec3(0.3, 0.55, 1.0) * rim * lit * 1.4;
}
