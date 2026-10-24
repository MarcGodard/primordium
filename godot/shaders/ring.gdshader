// Gold marker ring around a creature's base: a flat quad at its feet (the instance transform
// lays it down and sizes it to the body's footprint), soft ring, slow pulse. Drawn over
// everything: the rock is too bumpy for a flat ring to lie on without sinking in.
shader_type spatial;
render_mode unshaded, blend_add, depth_draw_never, depth_test_disabled, cull_disabled;

uniform vec4 tint : source_color = vec4(1.0, 0.8, 0.3, 1.0);

void fragment() {
	float r = length(UV - 0.5) * 2.0;
	float pulse = 0.75 + 0.25 * sin(TIME * 3.0);
	float ring = smoothstep(0.80, 0.87, r) * (1.0 - smoothstep(0.92, 0.98, r));
	// a faint glow inside the ring marks the ground the creature stands on
	float fill = (1.0 - smoothstep(0.0, 0.9, r)) * 0.08;
	ALBEDO = tint.rgb * 2.0;
	ALPHA = ring * pulse + fill;
}
