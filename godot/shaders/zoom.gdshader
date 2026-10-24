// Scale transition, powers of ten: a snapshot of the world being left. Climbing to a bigger
// scale it shrinks into a lens at the centre (the old world becomes a speck of the new one);
// diving it swells past the edges and dissolves. The new world shows wherever this is clear.
shader_type canvas_item;

uniform sampler2D snap : filter_linear;
// snapshot scale on screen: 1 = as it was, < 1 shrinking, > 1 swelling
uniform float s = 1.0;
// lens radius (fraction of screen height) the shrinking snapshot is seen through; 0 = no lens
uniform float lens = 0.0;
uniform float fade = 1.0;
uniform vec4 tint : source_color = vec4(0.5, 0.75, 1.0, 1.0);

void fragment() {
	float aspect = SCREEN_PIXEL_SIZE.y / SCREEN_PIXEL_SIZE.x;
	vec2 c = UV - 0.5;
	vec2 suv = c / s + 0.5;
	vec4 col = texture(snap, suv);
	float inside = step(0.0, suv.x) * step(suv.x, 1.0) * step(0.0, suv.y) * step(suv.y, 1.0);
	float a = inside;
	vec2 cr = vec2(c.x * aspect, c.y);
	float r = length(cr);
	if (lens > 0.0) {
		float m = 1.0 - smoothstep(lens * 0.92, lens, r);
		// a thin bright rim on the lens
		float rim = smoothstep(lens * 0.86, lens * 0.97, r) * (1.0 - smoothstep(lens, lens * 1.04, r));
		col.rgb = mix(col.rgb, tint.rgb, rim * 0.8);
		a = max(a * m, rim * 0.9);
	} else {
		// swelling: the edges go first, as if flying through
		a *= 1.0 - smoothstep(0.35, 0.75, r * (1.0 + (s - 1.0) * 0.3)) * smoothstep(1.0, 1.6, s);
	}
	COLOR = vec4(col.rgb, a * fade);
}
