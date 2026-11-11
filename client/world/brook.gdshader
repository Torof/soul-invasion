// A brook (BrookLines.mesh), in the same language as the river's bank:
// the ribbon is drawn wider than its water, and each vertex carries, as
// floats, how far from the brook's axis it lies across the ribbon (UV.x,
// tiles) and the water's half-width there (UV.y, tiles); as bytes, where
// its own tile's water runs (COLOR.rg) and the fade of a mouth into the
// river (COLOR.b). From the axis outwards: clear shallow water over its
// bed, a lapping edge, a thin band of wet earth, then the grass beneath
// shows past a ragged edge that overhangs the water in places.
shader_type canvas_item;

uniform sampler2D noise_texture : repeat_enable, filter_linear;
// The bed seen through the water, three matters laid side by side in
// soft patches a few metres across, and the bank's earth.
uniform sampler2D bed1_texture : repeat_enable, filter_linear_mipmap;
uniform sampler2D bed2_texture : repeat_enable, filter_linear_mipmap;
uniform sampler2D bed3_texture : repeat_enable, filter_linear_mipmap;
uniform float bed_patch = 3.0;
uniform sampler2D earth_texture : repeat_enable, filter_linear_mipmap;
// How much of the water's colour lies over the bed (a brook is a metre
// deep at most: the bed shows), and the water's tint.
uniform float water_over_bed = 0.58;
uniform vec3 water_tint : source_color = vec3(1.0);
uniform vec3 shallow_tint : source_color = vec3(0.88, 1.18, 1.18);
// The earth band: its width wanders between these, in tiles.
uniform float earth_min = 0.0;
uniform float earth_max = 0.1;
uniform float lap = 0.05;
uniform float rag = 0.07;
uniform float overhang = 0.1;
uniform vec3 ink : source_color = vec3(0.17, 0.12, 0.08);
// The period of the water's flow; it runs at its tile's own pace (the
// vertices carry the server's current, over ±flow_max metres a second).
uniform float flow_period = 2.0;
uniform float flow_max = 2.0;
// Development: the time the water has run for, pinned so that two
// captures can be compared pixel by pixel (SOUL_INVASION_WATER_TIME);
// negative, the clock's own.
uniform float flow_time = -1.0;
uniform float wet = 0.7;

varying vec4 tint;
varying vec2 across;
varying vec2 world;

void vertex() {
	tint = COLOR;
	across = UV;
	// The point of the map in tiles (a tile x spanning [x, x + 1)), from
	// the vertex's place on the screen (the inverse of BrookLines.to_screen
	// shifted half a tile: the ribbon's points are tile centres).
	vec2 px = (MODEL_MATRIX * vec4(VERTEX, 0.0, 1.0)).xy;
	world = vec2(px.x / 64.0 + (px.y + 16.0) / 32.0, (px.y + 16.0) / 32.0 - px.x / 64.0);
}

void fragment() {
	// The images are read with their mipmaps, a level sharper (as the
	// ground's): the derivatives, before a pixel can be discarded.
	vec2 uv = world * 0.25;
	vec2 dx = dFdx(uv) * 0.5;
	vec2 dy = dFdy(uv) * 0.5;
	float lateral = across.x;
	float half_width = across.y;
	float fade = tint.b;
	if (fade <= 0.0) {
		discard;
	}
	// Distance from the water's edge, positive on the land.
	float d = abs(lateral) - half_width;
	float present = smoothstep(0.02, 0.12, half_width);
	float wander = texture(noise_texture, world * 0.035 + vec2(0.27, 0.61)).r;
	float earth = mix(earth_min, earth_max, smoothstep(0.35, 0.7, wander)) * present;
	float ragged = (texture(noise_texture, world * 0.6 + vec2(0.4, 0.9)).r - 0.5) * 2.0 * rag
		+ (texture(noise_texture, world * 1.7 + vec2(0.1, 0.3)).r - 0.5) * rag;
	float outer = max(earth + ragged * clamp(earth / 0.08, 0.0, 1.0), 0.0);
	float tufts = texture(noise_texture, world * 2.1 + vec2(0.83, 0.47)).r * 0.7
		+ texture(noise_texture, world * 4.7 + vec2(0.27, 0.66)).r * 0.5;
	float over = overhang * clamp(tufts - 0.3, 0.0, 1.0) * present;
	float grass_at = mix(-over, outer, smoothstep(0.02, 0.1, outer));
	if (d > grass_at + 0.03 || half_width <= 0.0) {
		discard;
	}
	float lapping = (texture(noise_texture, world * 1.4 + vec2(0.55, 0.05)).r - 0.5) * 2.0 * lap;
	float dw = d - lapping * present;
	vec3 colour;
	if (dw < 0.0) {
		// The water drifts along its tile's own current, at its pace.
		vec2 drift = (tint.rg * 2.0 - 1.0) * flow_max * flow_period * 0.25;
		float now = flow_time < 0.0 ? TIME : flow_time;
		float t = now / flow_period;
		float p0 = fract(t);
		float w0 = 1.0 - abs(1.0 - 2.0 * p0);
		vec3 water = (textureGrad(TEXTURE, uv - drift * p0, dx, dy).rgb * w0
			+ textureGrad(TEXTURE, uv - drift * fract(t + 0.5) + vec2(0.5), dx, dy).rgb * (1.0 - w0)) * water_tint;
		float f = 0.2 / bed_patch;
		vec3 n = vec3(
			texture(noise_texture, world * f + vec2(0.21, 0.63)).r,
			texture(noise_texture, world * f + vec2(0.69, 0.27)).r,
			texture(noise_texture, world * f + vec2(0.43, 0.88)).r);
		vec3 w = pow(n, vec3(8.0));
		w /= max(w.x + w.y + w.z, 1e-5);
		vec2 b_uv = world * 0.25;
		vec3 bed = textureGrad(bed1_texture, b_uv, dx, dy).rgb * w.x
			+ textureGrad(bed2_texture, b_uv + vec2(0.37, 0.19), dx, dy).rgb * w.y
			+ textureGrad(bed3_texture, b_uv + vec2(0.71, 0.53), dx, dy).rgb * w.z;
		colour = mix(bed * shallow_tint * 1.15, water, water_over_bed);
		// Lighter towards the edges, where it is shallowest, the bed showing
		// more there.
		float shallow = 1.0 - smoothstep(0.0, 0.25, -dw);
		colour = mix(colour, mix(bed, colour, 0.5) * shallow_tint, shallow * 0.6);
		// Light glints drifting with the current.
		float g = texture(noise_texture, world * 1.3 + vec2(now * 0.05, now * 0.03)).r;
		colour = mix(colour, colour * 1.25, smoothstep(0.66, 0.74, g) * 0.5);
		colour *= 1.0 - (1.0 - smoothstep(0.0, 0.05, abs(dw + 0.03))) * 0.3;
	} else {
		colour = textureGrad(earth_texture, world * 0.25, dx, dy).rgb;
		colour *= mix(wet, 1.0, smoothstep(0.0, 0.08, dw));
	}
	float edge = 1.0 - smoothstep(0.0, 0.025, abs(d - grass_at + 0.012));
	colour = mix(colour, ink, edge * 0.8);
	float a = 1.0 - smoothstep(grass_at, grass_at + 0.03, d);
	COLOR = vec4(colour, a * fade);
}
