// The trees sway in the wind (Trees.draw). The world view draws trees and
// bodies on one canvas, so this shader is that canvas's: a tree is told
// apart by its modulate, alpha between 0.90 and 0.98, its red the tree's
// height (over 640 px) and its blue its own phase. Its foot stays put and
// its top leans: the two top corners of its image move, more at the top;
// gusts pass across the valley, so neighbours lean together.
shader_type canvas_item;

uniform sampler2D noise_texture : repeat_enable, filter_linear;
// The lean at the top, as a share of the tree's height, at rest and in a
// gust; how fast the tree rocks; how wide and slow the gusts are.
uniform float sway = 0.012;
uniform float gust = 0.03;
uniform float rock_speed = 1.1;
uniform float gust_scale = 0.0012;
uniform float gust_speed = 0.035;

varying float is_tree;

void vertex() {
	is_tree = (COLOR.a > 0.9 && COLOR.a < 0.98) ? 1.0 : 0.0;
	if (is_tree > 0.5) {
		float height = COLOR.r * 640.0;
		float phase = COLOR.b * 6.2831853;
		vec2 at = (MODEL_MATRIX * vec4(VERTEX, 0.0, 1.0)).xy;
		float wind = texture(noise_texture, at * gust_scale + vec2(TIME * gust_speed, 0.0)).r;
		float strength = sway + gust * smoothstep(0.45, 0.8, wind);
		float lean = sin(TIME * rock_speed + phase) * 0.6 + sin(TIME * rock_speed * 2.3 + phase * 1.7) * 0.25 + 0.35 * smoothstep(0.45, 0.8, wind);
		// UV.y is 0 at the top of the image, 1 at the foot.
		float up = 1.0 - UV.y;
		VERTEX.x += lean * strength * height * up;
		VERTEX.y += abs(lean) * strength * height * up * 0.15;
	}
}

void fragment() {
	if (is_tree > 0.5) {
		COLOR = texture(TEXTURE, UV);
	}
}
