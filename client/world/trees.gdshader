// The trees sway in the wind (Trees.draw). The world view draws trees and
// bodies on one canvas, so this shader is that canvas's: a tree is told
// apart by its modulate, alpha between 0.90 and 0.98, its red the tree's
// height (over 1280 px) and its blue its own phase. The tree bends rather
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


// The ring of outline chunks around the view (ServerMessage::ChunkOutline)
// loses its detail with the distance from the view: 0 inside the view,
// 1 at the ring's outer edge, where the far view takes over (FarView).
// Set once for every shader by WorldView (project.godot, shader_globals):
// the detailed chunks' bounds in tiles, and the ring's width.
global uniform vec2 detail_lo;
global uniform vec2 detail_hi;
global uniform float detail_fade;
float fade_at_tile(vec2 t) {
	vec2 o = max(detail_lo - t, t - detail_hi);
	return clamp(max(o.x, o.y) / detail_fade, 0.0, 1.0);
}
float fade_at_screen(vec2 p) {
	return fade_at_tile(vec2((p.x / 32.0 + p.y / 16.0) * 0.5, (p.y / 16.0 - p.x / 32.0) * 0.5));
}

varying float is_tree;
varying float far;
varying vec2 local;
varying float front_of_body;
varying float hides_mouse;
varying float bend_px;
varying float width_px;
varying float pad;
varying float seed;

void vertex() {
	is_tree = (COLOR.a > 0.9 && COLOR.a < 0.98) ? 1.0 : 0.0;
	// Faded by where it stands: a tree by its foot (its node's place), a
	// plant of a chunk's layer by its own corner.
	far = fade_at_screen((MODEL_MATRIX * vec4(is_tree > 0.5 ? vec2(0.0) : VERTEX, 0.0, 1.0)).xy);
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
		float wind = smoothstep(0.45, 0.8, texture(noise_texture, at * gust_scale + vec2(TIME * gust_speed, 0.0)).r);
		float rock = sin(TIME * rock_speed + phase) * 0.7 + sin(TIME * rock_speed * 1.9 + phase * 1.7) * 0.3;
		bend_px = (rock * (sway + gust * wind) + gust * wind * 0.5) * height;
		// The image's width, from its height and the texture's proportions.
		vec2 size = 1.0 / TEXTURE_PIXEL_SIZE;
		width_px = height * size.x / size.y;
		// Widen the quad either side by the largest bend.
		pad = (sway + gust * 1.5) * height + 1.0;
		VERTEX.x += (UV.x * 2.0 - 1.0) * pad;
		// Where this point stands in the world view (the tree node is placed
		// at its foot).
		local = (MODEL_MATRIX * vec4(VERTEX, 0.0, 1.0)).xy;
	}
}

// A little muted across the ring (the far view's haze rises over it) and
// gone at its outer edge: the far land shows no trees, only woods.
vec4 fade_colour(vec4 c) {
	c.rgb = mix(c.rgb, vec3(dot(c.rgb, vec3(0.333))), 0.3 * far);
	c.a *= 1.0 - smoothstep(0.7, 1.0, far);
	return c;
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
		// Clear pixels are discarded: no blending with what lies beneath.
		if (uv.x < 0.0 || uv.x > 1.0) {
			discard;
		} else {
			// One mipmap level sharper than the hardware picks: sharp up close,
			// still no shimmer far away.
			// Far in the ring, a blurrier level of the image.
			vec4 c = texture(TEXTURE, uv, mip_bias + 2.5 * far);
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
				float body_on = front_of_body * body_reveal;
				float mouse_on = hides_mouse * mouse_reveal;
				if (body_on + mouse_on > 0.0) {
					float in_body = (1.0 - smoothstep(body_radius - see_rim, body_radius, distance(local, body_at))) * body_on;
					float in_mouse = (1.0 - smoothstep(mouse_radius - see_rim, mouse_radius, distance(local, mouse_at))) * mouse_on;
					c.a *= 1.0 - max(in_body, in_mouse) * (1.0 - see_through);
				}
				COLOR = fade_colour(c);
			}
		}
	} else if (far > 0.0) {
		// Plants and anything else standing: blurred and muted the same.
		COLOR = fade_colour(texture(TEXTURE, UV, 2.5 * far) * COLOR);
	}
}
