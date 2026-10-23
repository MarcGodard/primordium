// Scale transition: streaks rush from (inward) or toward (outward) the centre while an iris
// closes to dark, then opens on the next scale. `p` 0..1..0 is driven by the tween.
shader_type canvas_item;

uniform float p = 0.0;
uniform float dir = 1.0;
uniform vec4 tint : source_color = vec4(0.4, 0.7, 1.0, 1.0);

float hash(float x) { return fract(sin(x * 127.1) * 43758.5453); }

void fragment() {
	vec2 uv = SCREEN_UV - 0.5;
	uv.x *= SCREEN_PIXEL_SIZE.y / SCREEN_PIXEL_SIZE.x;
	float r = length(uv);
	float a = atan(uv.y, uv.x);
	float lanes = 240.0;
	float u = a / 6.2831 * lanes;
	float lane = floor(u);
	float across = smoothstep(0.25, 0.0, abs(fract(u) - 0.5));
	float speed = 0.6 + hash(lane) * 1.4;
	float head = fract(hash(lane + 7.0) + TIME * speed * dir) * 0.95;
	// a thin streak with a tail trailing behind its head
	float along = dir > 0.0 ? head - r : r - head;
	float streak = across * smoothstep(0.0, 0.01, along + 0.01) * smoothstep(0.12, 0.0, along) * step(0.6, hash(lane + 3.0)) * smoothstep(0.03, 0.15, r);
	float iris = smoothstep(0.75 * (1.0 - p), 0.75 * (1.0 - p) + 0.2, r);
	vec3 col = tint.rgb * streak * p * 1.6;
	float dark = clamp(iris * p * 1.4 + p * p * 0.9, 0.0, 1.0);
	COLOR = vec4(col, max(dark, streak * p));
}
