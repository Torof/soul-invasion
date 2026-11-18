// The trees sway in the wind (Trees.draw). The world view draws trees and
// bodies on one canvas, so this shader is that canvas's: a tree is told
// apart by its modulate, alpha between 0.90 and 0.98, its red the tree's
// height (over 1280 px) and its blue its own phase. The tree bends rather
// than leans: the lower part of its trunk stays still, the bend grows
// towards the top. Gusts pass across the valley, so neighbours bend
// together. The polygon the tree is drawn as leaves room either side of
// the image for the bent crown (its UVs run past 0..1 there).
// A tree's fruit (D48, Trees.Stand._draw_fruit) is drawn after it, alpha
// 0.80 to 0.86 (0.02 a see-through flag), red its tree's height, green how
// high it hangs on it (0 the foot, 1 the top), blue its tree's phase: it
// moves with the crown there, whole.
shader_type canvas_item;

uniform sampler2D noise_texture : repeat_enable, filter_linear;
// The bend at the top, as a share of the tree's height, at rest and in a
// gust; how fast the tree rocks; how wide and slow the gusts are; the
// share of the height, from the foot, that never moves. The tree is drawn
// as a polygon around what its image shows (Trees.Stand, TreeOutline),
// with room either side for the largest bend: sway, gust and still are
// Trees.SWAY, GUST and STILL, which size that room.
uniform float sway = 0.005;
uniform float gust = 0.012;
uniform float rock_speed = 0.55;
uniform float gust_scale = 0.0012;
uniform float gust_speed = 0.018;
uniform float still = 0.3;
// Mipmap bias: negative reads a sharper level.
uniform float mip_bias = -1.0;
// The foot melts into the ground: the lower foot_dark of the tree darkens
// (the shade among the roots), the lowest foot_melt dissolves raggedly,
// tinted towards the earth.
uniform float foot_dark = 0.16;
uniform float foot_melt = 0.05;
uniform vec3 earth : source_color = vec3(0.30, 0.27, 0.16);

// Seeing through: a tree in front of the piloted body fades within a
// circle around it, and a tree hiding something under the mouse within a
// circle around the mouse. Centres and radii in the world view's pixels;
// the strengths ease in and out (the world view sets them every frame).
uniform vec2 body_at = vec2(-1e6);
uniform float body_radius = 96.0;
uniform float body_reveal = 0.0;
uniform vec2 mouse_at = vec2(-1e6);
uniform float mouse_radius = 77.0;
uniform float mouse_reveal = 0.0;
// What is left of a tree inside a circle, and the soft rim, in pixels.
uniform float see_through = 0.25;
uniform float see_rim = 18.0;

varying float is_tree;
varying float is_fruit;
varying vec2 local;
varying float front_of_body;
varying float hides_mouse;
varying float bend_px;
varying float width_px;
varying float seed;

// How far the top of a tree `height` px tall, of phase `phase`, standing
// at `at`, is bent at `time`.
float bend_at(vec2 at, float height, float phase, float time) {
	float wind = smoothstep(0.45, 0.8, texture(noise_texture, at * gust_scale + vec2(time * gust_speed, 0.0)).r);
	float rock = sin(time * rock_speed + phase) * 0.7 + sin(time * rock_speed * 1.9 + phase * 1.7) * 0.3;
	return (rock * (sway + gust * wind) + gust * wind * 0.5) * height;
}

void vertex() {
	is_tree = (COLOR.a > 0.9 && COLOR.a < 0.98) ? 1.0 : 0.0;
	is_fruit = (COLOR.a > 0.79 && COLOR.a < 0.87) ? 1.0 : 0.0;
	if (is_fruit > 0.5) {
		float flags = round((COLOR.a - 0.8) / 0.02);
		front_of_body = mod(flags, 2.0);
		hides_mouse = floor(flags / 2.0);
		vec2 at = (MODEL_MATRIX * vec4(VERTEX, 0.0, 1.0)).xy;
		// The crown at the fruit's height moves the image right by the bend
		// there (the tree's fragment reads it from that far left).
		float k = clamp((COLOR.g - still) / (1.0 - still), 0.0, 1.0);
		VERTEX.x += bend_at(at, COLOR.r * 1280.0, COLOR.b * 6.2831853, TIME) * k * k;
		local = at;
	}
	if (is_tree > 0.5) {
		float height = COLOR.r * 1280.0;
		float phase = COLOR.b * 6.2831853;
		seed = COLOR.b;
		// Green: 0.25 nothing, +0.25 in front of the body, +0.5 hiding what
		// is under the mouse.
		float flags = round((COLOR.g - 0.25) * 4.0);
		front_of_body = mod(flags, 2.0);
		hides_mouse = floor(flags / 2.0);
		vec2 at = (MODEL_MATRIX * vec4(VERTEX, 0.0, 1.0)).xy;
		bend_px = bend_at(at, height, phase, TIME);
		// The image's width, from its height and the texture's proportions.
		vec2 size = 1.0 / TEXTURE_PIXEL_SIZE;
		width_px = height * size.x / size.y;
		// Where this point stands in the world view (the tree's item is
		// placed at its foot).
		local = at;
	}
}

// See through a tree (and its fruit) within the circles that apply to it
// (its flags, standing at `here`): what is left of `alpha`.
float seen(float alpha, float in_front, float hides, vec2 here) {
	float body_on = in_front * body_reveal;
	float mouse_on = hides * mouse_reveal;
	if (body_on + mouse_on > 0.0) {
		float in_body = (1.0 - smoothstep(body_radius - see_rim, body_radius, distance(here, body_at))) * body_on;
		float in_mouse = (1.0 - smoothstep(mouse_radius - see_rim, mouse_radius, distance(here, mouse_at))) * mouse_on;
		return alpha * (1.0 - max(in_body, in_mouse) * (1.0 - see_through));
	}
	return alpha;
}

void fragment() {
	if (is_fruit > 0.5) {
		vec4 c = texture(TEXTURE, UV, mip_bias);
		if (c.a <= 0.0) {
			discard;
		}
		c.a = seen(c.a, front_of_body, hides_mouse, local);
		COLOR = c;
	} else if (is_tree > 0.5) {
		// The image's own coordinates (the polygon's UVs), in pixels, then
		// bent: more towards the top, nothing over the still lower part.
		float x = UV.x * width_px;
		float up = 1.0 - UV.y;
		float k = clamp((up - still) / (1.0 - still), 0.0, 1.0);
		x -= bend_px * k * k;
		vec2 uv = vec2(x / width_px, UV.y);
		// One mipmap level sharper than the hardware picks: sharp up close,
		// still no shimmer far away. Read before any pixel is discarded: the
		// card picks the level from neighbouring pixels, and next to
		// discarded ones (past the image's side) it picked a tiny level,
		// whose blur of the crown drew a thin dark line along the image's
		// edge, swaying with the tree.
		vec4 c = texture(TEXTURE, uv, mip_bias);
		// Clear pixels are discarded: no blending with what lies beneath.
		if (uv.x < 0.0 || uv.x > 1.0) {
			discard;
		} else {
			// Nothing more to do where the image is clear (between the
			// branches, around the crown): most of the quad.
			if (c.a <= 0.0) {
				discard;
			} else {
				float low = UV.y;
				// Shade among the roots.
				float dark = smoothstep(1.0 - foot_dark, 1.0, low);
				c.rgb *= 1.0 - dark * 0.35;
				// The last few percent melt into the ground, raggedly; only
				// there (worked out over the whole tree, the noise thinned
				// random specks of the crown).
				float melt = smoothstep(1.0 - foot_melt, 1.0, low);
				if (melt > 0.0) {
					float n = texture(noise_texture, vec2(uv.x * 3.0 + seed * 7.0, low * 9.0)).r * 0.6
						+ texture(noise_texture, vec2(uv.x * 11.0 + seed * 3.0, low * 23.0)).r * 0.4;
					c.rgb = mix(c.rgb, earth, melt * 0.6);
					c.a *= 1.0 - smoothstep(n - 0.15, n + 0.15, melt);
				}
				// See through it, within the circles that apply to it: only
				// for the few trees flagged, while a circle is open.
				c.a = seen(c.a, front_of_body, hides_mouse, local);
				COLOR = c;
			}
		}
	}
}
