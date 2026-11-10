shader_type canvas_item;
// The haze over the far land, in tile coordinates (UV): none well inside
// the ground held, rising to its edge, thickening far away. FarView sets
// every value below from its constants; the defaults are the same.
uniform vec2 held_lo = vec2(0.0);
uniform vec2 held_hi = vec2(0.0);
uniform vec3 haze = vec3(0.62, 0.65, 0.66);
uniform float edge = 0.5;
uniform float far_haze = 0.68;
uniform float rise_in = 24.0;
uniform float reach = 700.0;
// The radius of the rounded corners of the area held, in tiles.
uniform float corner = 0.0;
// Development: the time the fog has drifted for, pinned so that two
// captures can be compared pixel by pixel (SOUL_INVASION_FOG_TIME);
// negative, the clock's own.
uniform float drift_time = -1.0;
uniform sampler2D noise_texture : repeat_enable, filter_linear;

void fragment() {
	vec2 t = UV;
	// A veil, then a fog: it starts where the view's last tiles begin (by
	// up to half a tile sooner, wandering and drifting slowly), is whole
	// at the view's limit, and comes in soft lighter and darker banks.
	float time = drift_time >= 0.0 ? drift_time : TIME;
	vec2 drift = vec2(time * 0.25, time * 0.1);
	float n = texture(noise_texture, (t + drift) / 14.0).r;
	float m = texture(noise_texture, (t - drift * 0.6) / 37.0 + vec2(0.31, 0.77)).r;
	// Distance outside the area held, negative inside: a square with its
	// corners rounded to `corner` tiles, so the fog turns them in an arc
	// (the project lead's call), not at a right angle.
	vec2 half_size = (held_hi - held_lo) * 0.5;
	float r = min(corner, min(half_size.x, half_size.y));
	vec2 q = abs(t - (held_lo + held_hi) * 0.5) - (half_size - vec2(r));
	float d = length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - r + n * 0.5;
	// Nothing over the ground held (d <= 0): the haze starts at its edge.
	float a = smoothstep(0.0, rise_in, d) * edge + smoothstep(rise_in, reach, d) * (far_haze - edge);
	COLOR = vec4(haze * (0.94 + 0.1 * m), a);
}
