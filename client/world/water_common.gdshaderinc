// What the two water shaders share (WaterMeshes): the flowing water, the
// ford's bed, and the dials, the same names as ground.gdshader's so
// ground.json sets both.

// The water flows: the painted water advected along the current in two
// phases half a period apart, cross-faded by a triangle wave so the reset
// never shows. Read at an integer mipmap level chosen once a zoom step
// (water_lod): a bilinear read, never a trilinear one, and no shimmer
// from a level that changes with the camera.
uniform float flow_period = 2.4;
uniform float water_lod = 0.0;
// A second layer of the same water, turned and smaller, over the first
// by flow_mix (today's two layers): hides the four-metre repeat for two
// more reads. Zero: one layer.
uniform float flow_mix = 0.45;
// A slow, still warp of the water's coordinates, so its four-metre
// lattice no longer lines up with itself: in periods of the texture,
// over about six tiles. The flowing pattern passes through it as water
// over an uneven bed. Zero: none.
uniform float water_warp = 0.0;
// Metres the water's texture repeats over (4; the 8 m water on F12).
uniform float water_metres = 4.0;
// The current as a layer of light: a grey streak map (mid-grey no
// change) read with its streaks along the flow and scrolled at the
// current's pace, in two phases like the water; the water's colour times
// 1 + streak_strength × (streak - 0.5). F11.
uniform sampler2D streak_texture : repeat_enable, filter_linear_mipmap;
uniform bool has_streaks = false;
uniform float streak_metres = 4.0;
uniform float streak_strength = 1.5;
uniform float streak_speed = 1.0;
// The ground's images (the beds, the earth, the sand, the detail) read at
// their level a zoom step.
uniform float ground_lod = 0.0;
uniform vec3 base_tint : source_color = vec3(1.0);
// Darker and cooler towards the middle of the river, over deep_reach
// tiles from the bank; lighter within field_light_reach of it.
uniform vec3 deep_tint : source_color = vec3(0.72, 0.86, 0.92);
uniform float deep_reach = 2.5;
uniform float field_light_reach = 0.9;
uniform float field_light = 0.55;
uniform vec3 shore_tint : source_color = vec3(1.35, 1.3, 1.1);
// The depth the vertices carry: tiles from the line, over this many
// (WaterMeshes.DEPTH_TILES).
uniform float depth_tiles = 3.0;
// A ford: its bed under the clear shallow water, how much of the water's
// colour stays over it, the water thickening over ford_shelf tiles, the
// ripples over the stones.
uniform sampler2D bed_texture : repeat_enable, filter_linear_mipmap;
uniform bool has_bed = false;
uniform float ford_water = 0.42;
uniform float ford_shelf = 1.6;
uniform vec3 ford_tint : source_color = vec3(0.88, 1.18, 1.18);
uniform float ford_ripple = 0.35;
// The bank's detail, painted once (WaterMeshes.detail_texture): R the
// grass edge's rag, G its tufts, B the water's lap, A the ripples' grain,
// four metres a texture, read on the band only and from zoom 0.8 up
// (detail_on), where its shapes are more than a few pixels.
uniform sampler2D detail_texture : repeat_enable, filter_linear_mipmap;
uniform float detail_on = 1.0;
// Development: the time the water has run for, pinned so that two
// captures can be compared pixel by pixel (SOUL_INVASION_WATER_TIME);
// negative, the clock's own.
uniform float flow_time = -1.0;
// Development: the screen split against today's water (F7): this water
// draws on the right of split_edge (0 to 1 across the screen); negative,
// everywhere.
uniform float split_edge = -1.0;

// The current the vertex colour carries, in tiles a second.
vec2 flow_of(vec4 tile) {
	return (tile.rg * 2.0 - 1.0) * 2.0;
}

// One layer of the flowing water: the texture's coordinates `base`,
// drifting by `f` a period, turned by `turn`.
vec3 flowing(sampler2D tex, vec2 base, vec2 f, float period, float shift, float now) {
	float t = now / period + shift;
	float p0 = fract(t);
	float p1 = fract(t + 0.5);
	float w0 = 1.0 - abs(1.0 - 2.0 * p0);
	vec3 a = textureLod(tex, base - f * p0 + vec2(shift, 0.0), water_lod).rgb;
	vec3 b = textureLod(tex, base - f * p1 + vec2(0.0, shift) + vec2(0.5), water_lod).rgb;
	return a * w0 + b * (1.0 - w0);
}

// The flowing water at a point of the map in tiles.
vec3 water_at(sampler2D tex, vec2 world, vec2 runs, float now) {
	vec2 base = world / water_metres;
	if (water_warp > 0.0) {
		// Two waves of unrelated periods each way, so the warp itself has
		// no visible stripes.
		base += water_warp * vec2(
			sin(world.x * 0.9 + world.y * 0.37) + 0.6 * sin(world.x * 0.31 - world.y * 0.83),
			cos(world.y * 1.1 - world.x * 0.43) + 0.6 * cos(world.x * 0.57 + world.y * 0.29));
	}
	vec3 colour = flowing(tex, base, runs * (flow_period / water_metres), flow_period, 0.0, now);
	if (flow_mix > 0.0) {
		mat2 turn = mat2(vec2(0.0, 1.0), vec2(-1.0, 0.0));
		colour = mix(colour, flowing(tex, turn * base * 1.6, turn * runs * (1.15 * flow_period * 1.3 * 1.6 / water_metres), flow_period * 1.3, 0.37, now), flow_mix);
	}
	if (has_streaks) {
		float pace = length(runs);
		vec2 dir = pace > 1e-4 ? runs / pace : vec2(0.0, 1.0);
		// The map's rows across the flow, its columns along it.
		vec2 s = vec2(dot(world, vec2(dir.y, -dir.x)), dot(world, dir)) / streak_metres;
		float t = now / flow_period;
		float p0 = fract(t);
		float p1 = fract(t + 0.5);
		float w0 = 1.0 - abs(1.0 - 2.0 * p0);
		float drift = pace * streak_speed * flow_period / streak_metres;
		float l0 = textureLod(streak_texture, s - vec2(0.0, drift * p0), water_lod).r;
		float l1 = textureLod(streak_texture, s - vec2(0.0, drift * p1) + vec2(0.5, 0.0), water_lod).r;
		colour *= 1.0 + streak_strength * (l0 * w0 + l1 * (1.0 - w0) - 0.5);
	}
	return colour;
}

// The water over a ford's bed: `colour` the water, `depth` how far into
// the water in tiles, `raw` the ford's value (smoothed wide), `ford_bank`
// how much the bank here is a ford's (0 on the deep mesh).
vec3 ford_over_bed(vec3 colour, vec2 world, float depth, float raw, float ford_bank, float now) {
	// Into the deep water the bed fades out slowly.
	float ford = smoothstep(0.02, 0.97, raw);
	ford = ford * ford * (3.0 - 2.0 * ford);
	// Near a ford's bank the water is all ford, so its film meets the bed
	// on the land without a line.
	ford = max(ford, ford_bank * (1.0 - smoothstep(0.0, ford_shelf, depth)));
	if (ford <= 0.0) {
		return colour;
	}
	vec3 bed = textureLod(bed_texture, world * 0.25, ground_lod).rgb;
	// From the bank inwards, the water over the bed thickens; at the edge
	// of a ford's bank it is a film that thins to nothing on the bed.
	float shelf = smoothstep(0.0, ford_shelf, depth);
	float deep = mix(mix(0.18, 0.0, ford_bank), ford_water, shelf);
	vec3 wet_bed = bed * mix(vec3(0.85), ford_tint * 1.35, mix(1.0, shelf, ford_bank));
	vec3 shallow = mix(wet_bed, colour, deep);
	// Ripples drifting with the current over the stones: the detail's
	// grain, one read.
	float ripple = smoothstep(0.62, 0.72, textureLod(detail_texture, world * 0.25 + vec2(now * 0.0075, now * 0.0125), ground_lod).a * 1.1);
	shallow = mix(shallow, shallow * shore_tint * 1.15, ripple * ford_ripple);
	return mix(colour, shallow, ford);
}
