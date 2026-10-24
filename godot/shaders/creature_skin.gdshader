// A creature's skin: the body's own colours (vertex colour, by what its cells do) with a gold
// glow along its silhouette that pulses. Edge-on glow reads from any side, unlike a ring.
shader_type spatial;
render_mode cull_back;

uniform vec4 glow : source_color = vec4(1.0, 0.78, 0.25, 1.0);

void fragment() {
	ALBEDO = COLOR.rgb;
	ROUGHNESS = 0.22;
	SPECULAR = 0.7;
	float edge = pow(1.0 - clamp(dot(normalize(NORMAL), normalize(VIEW)), 0.0, 1.0), 2.5);
	float pulse = 0.65 + 0.35 * sin(TIME * 3.0);
	EMISSION = glow.rgb * edge * pulse * 2.2;
}
