// The trees sway in the wind (Trees.draw). The world view draws trees and
// bodies on one canvas, so this shader is that canvas's: a tree is told
// apart by its modulate, alpha between 0.90 and 0.98, its red the tree's
// height (over 640 px) and its blue its own phase. The tree bends rather
// than leans: the lower part of its trunk stays still, the bend grows
// towards the top. Gusts pass across the valley, so neighbours bend
// together. The quad is widened a little so the bent crown is not cut.
shader_type canvas_item;

uniform sampler2D noise_texture : repeat_enable, filter_linear;
// The bend at the top, as a share of the tree's height, at rest and in a
// gust; how fast the tree rocks; how wide and slow the gusts are; the
// share of the height, from the foot, that never moves.
uniform float sway = 0.005;
uniform float gust = 0.012;
uniform float rock_speed = 0.55;
uniform float gust_scale = 0.0012;
uniform float gust_speed = 0.018;
uniform float still = 0.3;

varying float is_tree;
varying float bend_px;
varying float width_px;
varying float pad;

void vertex() {
	is_tree = (COLOR.a > 0.9 && COLOR.a < 0.98) ? 1.0 : 0.0;
	if (is_tree > 0.5) {
		float height = COLOR.r * 640.0;
		float phase = COLOR.b * 6.2831853;
		vec2 at = (MODEL_MATRIX * vec4(VERTEX, 0.0, 1.0)).xy;
		float wind = smoothstep(0.45, 0.8, texture(noise_texture, at * gust_scale + vec2(TIME * gust_speed, 0.0)).r);
		float rock = sin(TIME * rock_speed + phase) * 0.7 + sin(TIME * rock_speed * 1.9 + phase * 1.7) * 0.3;
		bend_px = (rock * (sway + gust * wind) + gust * wind * 0.5) * height;
		// The image's width, from its height and the texture's proportions.
		vec2 size = 1.0 / TEXTURE_PIXEL_SIZE;
		width_px = height * size.x / size.y;
		// Widen the quad either side by the largest bend.
		pad = (sway + gust * 1.5) * height + 1.0;
		VERTEX.x += (UV.x * 2.0 - 1.0) * pad;
	}
}

void fragment() {
	if (is_tree > 0.5) {
		// Back to the image's own coordinates, then bent: more towards the
		// top, nothing over the still lower part.
		float x = UV.x * (width_px + 2.0 * pad) - pad;
		float up = 1.0 - UV.y;
		float k = clamp((up - still) / (1.0 - still), 0.0, 1.0);
		x -= bend_px * k * k;
		vec2 uv = vec2(x / width_px, UV.y);
		if (uv.x < 0.0 || uv.x > 1.0) {
			COLOR = vec4(0.0);
		} else {
			COLOR = texture(TEXTURE, uv);
		}
	}
}
