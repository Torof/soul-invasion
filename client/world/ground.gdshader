// The ground: one seamless top-down texture per terrain, projected onto
// the diamonds (docs/09-visual-production.md). Its four-metre repetition
// is broken by blending it with a sister image over soft noise and, where
// wanted, by laying it in shifted variants chosen by zones of noise. The
// textures are never turned: drawn with a slight three-quarter reading,
// a turned grass reads upside down, like a roof.
shader_type canvas_item;

uniform sampler2D noise_texture : repeat_enable, filter_linear;
// Noise period, in texture units (one unit is one texture, four metres).
uniform float noise_scale = 0.09;
// Period of the zones where one shifted variant is laid: wide and few,
// so the ground stays calm; the cuts between them are short.
uniform float variant_scale = 0.05;
// Where the shifted copies are taken from, in texture units.
uniform vec2 second_offset = vec2(0.37, 0.63);
// Whether the textures are laid in shifted variants (the woods) or as
// they are (the meadow, whose sister already breaks the repetition).
uniform bool has_variants = false;
// A sister image of the ground (same grass, drawn differently), blended
// with the first over slow noise.
uniform sampler2D sibling_texture : repeat_enable, filter_linear;
uniform bool has_sibling = false;
// Share of the ground the sibling covers, and a tint to bring its tone
// to the first image's when the two differ (white: none).
uniform float sibling_cover = 0.5;
uniform vec3 sibling_tint : source_color = vec3(1.0);
// Share of the base's bare spots (its brown pixels) kept; the others
// show the sibling's grass instead. Needs a sibling.
uniform float bare_keep = 1.0;
// Up to two more compatible textures of the same terrain (made with the
// first as style reference), each shown in soft patches of its own:
// "second" over wide areas, "worn" in a few spots.
uniform sampler2D second_texture : repeat_enable, filter_linear;
uniform bool has_second = false;
uniform float second_scale = 0.03;
uniform float second_cover = 0.35;
// Width of the fade at a patch's edge (0.1 is a crisp spot; 0.3 or more
// melts one water into another).
uniform float second_softness = 0.1;
uniform sampler2D worn_texture : repeat_enable, filter_linear;
uniform bool has_worn = false;
// Period of the patches, in texture units, and how much ground they take.
uniform float worn_scale = 0.05;
uniform float worn_cover = 0.15;
uniform float worn_softness = 0.1;
// Tone variation without another image: the same texture, tinted a
// little (drier, lighter) over wide slow zones. Same tufts by
// construction; strength and pace are numbers.
uniform vec3 dry_tint : source_color = vec3(1.07, 1.03, 0.86);
uniform float dry_scale = 0.018;
uniform float dry_cover = 0.5;
uniform float dry_strength = 0.6;
uniform float dry_softness = 0.3;
// The terrain's own tone over everything: white for the meadow, darker
// and cooler for the shade of the woods.
uniform vec3 base_tint : source_color = vec3(1.0);
// Where a terrain spills over its neighbour, the vertex alpha fades
// across the border; a noise read at this period erodes that fade so the
// edge is ragged, not straight.
uniform float edge_scale = 1.5;
// Water lightens near its banks: when shore_lighten is above zero, the
// vertex colour's green channel carries 1 - nearness of the bank, and the
// ground is tinted by shore_tint by that much, ragged by the same noise.
uniform float shore_lighten = 0.0;
uniform vec3 shore_tint : source_color = vec3(1.35, 1.3, 1.1);
// A thin darker line at the very edge of the water (the wet bank).
uniform float bank_line = 0.0;
// The bank itself: where the water spills over the land (vertex alpha
// below 1), the shader lays the bank strip instead of water: x across
// the bank (0 at the water), y along the river; its outer edge eroded
// like any spill.
uniform sampler2D bank_texture : repeat_enable, filter_linear;
uniform bool has_bank = false;
// A second strip of another matter (earth beside sand), chosen over
// slow stretches along the river so the bank changes.
uniform sampler2D bank2_texture : repeat_enable, filter_linear;
uniform bool has_bank2 = false;
// A third matter (pebbles), for the bank drawn from the field only.
uniform sampler2D bank3_texture : repeat_enable, filter_linear;
uniform bool has_bank3 = false;
// The bank's width across the spill varies along the river between
// bank_reach_min and bank_reach (1 = the whole spill); beyond it the land
// shows. bank_vary is the period of that variation along the river.
uniform float bank_reach = 1.0;
uniform float bank_reach_min = 0.4;
// Width of the water's spill over the land, in tiles (the fringe's
// alpha fades over it), so distances across the bank are in tiles.
uniform float bank_spill = 1.0;
// Below this wander the bank vanishes and the grass meets the water;
// bank_gate_width is how softly it comes back.
uniform float bank_gate = 0.3;
uniform float bank_gate_width = 0.2;
uniform float bank_vary = 0.3;
// How far the bank bites into the water at most, in tiles, and the
// scale of the fine grain that roughens its two edges.
uniform float bank_into = 0.6;
uniform float bank_edge = 9.0;
// A terrain whose spill exists only for its bank (water) spills nothing
// when it has no bank texture.
uniform bool spill_only_with_bank = false;
// The current: soft lighter streaks stretched along the river's axis
// (the texture's y), drifting with time. Drawn by the shader, so they
// never repeat as a tile would and they move. Zero turns them off.
uniform float current_strength = 0.0;
uniform float current_scale = 3.0;
uniform float current_stretch = 6.0;
uniform float current_speed = 0.04;
uniform float current_cover = 0.3;

// The bank drawn from the distance to the water's edge (ShoreField):
// the water canvas also covers the land within two tiles of it; there the
// shader lays, from the water outwards, a lighter water, the ink line of
// the edge, a band of earth whose width wanders along the bank, and lets
// the grass beneath show past a ragged edge.
uniform bool has_field = false;
uniform sampler2D shore_field : filter_linear, repeat_disable;
// Tiles the field covers, and the distance range it encodes either way.
uniform float field_tiles = 256.0;
uniform float field_range = 4.0;
// The band of earth: its width in tiles wanders between these over
// stretches of about field_vary tiles; the grass edge is ragged over
// field_rag tiles.
uniform float field_width_min = 0.0;
uniform float field_width_max = 0.55;
uniform float field_vary = 9.0;
uniform float field_rag = 0.16;
// The ink line at the water's edge, its half-width in tiles.
uniform float field_line = 0.035;
uniform vec3 field_ink : source_color = vec3(0.17, 0.12, 0.08);
// How far into the water it lightens, in tiles, and how much.
uniform float field_light_reach = 0.9;
uniform float field_light = 0.55;
// The earth's own tone, and how much darker it is where wet and under
// the grass's overhang.
uniform vec3 field_earth_tint : source_color = vec3(1.0);
uniform float field_wet = 0.7;
uniform float field_shade = 0.75;
// Beaches: over stretches of about field_beach tiles the earth gives way
// to sand (bank2) or pebbles (bank3), each covering about field_beach_cover
// of the bank, and the bank is wider there by field_beach_wide.
uniform float field_beach = 14.0;
uniform float field_beach_cover = 0.25;
uniform float field_beach_wide = 1.6;

// Soft patches covering about `cover` of the ground, from the noise read
// at `scale` with an offset so each layer has its own shape.
float patches(vec2 uv, float scale, vec2 shift, float cover, float softness) {
	float m = texture(noise_texture, uv * scale + shift).r;
	float threshold = 1.0 - cover;
	return smoothstep(threshold - softness, threshold + softness, m);
}

// A texture laid in four variants shifted (never turned), chosen by three
// soft noises, so the four-metre grid is out of phase from one zone to
// the next. The built-in TEXTURE cannot go through a sampler parameter,
// so its sampling is written out where it is used and only the
// coordinates and weights are shared.
vec2 uv_b(vec2 uv) { return uv + second_offset; }
vec2 uv_c(vec2 uv) { return uv + vec2(0.11, 0.47); }
vec2 uv_d(vec2 uv) { return uv + vec2(0.73, 0.29); }
vec3 variant_weights(vec2 uv) {
	if (!has_variants) {
		return vec3(0.0);
	}
	float n1 = texture(noise_texture, uv * variant_scale).r;
	float n2 = texture(noise_texture, uv * variant_scale * 0.7 + vec2(0.41, 0.17)).r;
	float n3 = texture(noise_texture, uv * variant_scale * 1.3 + vec2(0.89, 0.61)).r;
	return vec3(smoothstep(0.47, 0.53, n1), smoothstep(0.47, 0.53, n2), smoothstep(0.47, 0.53, n3));
}
vec4 blend4(vec4 a, vec4 b, vec4 c, vec4 d, vec3 w) {
	return mix(mix(a, b, w.x), mix(c, d, w.z), w.y);
}
vec4 varied(sampler2D tex, vec2 uv) {
	return blend4(texture(tex, uv), texture(tex, uv_b(uv)), texture(tex, uv_c(uv)), texture(tex, uv_d(uv)), variant_weights(uv));
}

// The vertex colour alone: white under a texture, the flat colour of a
// terrain without one, the colour of an item's disc.
varying vec4 tint;

void vertex() {
	tint = COLOR;
}

void fragment() {
	vec2 uv = UV;
	vec4 ground = blend4(texture(TEXTURE, uv), texture(TEXTURE, uv_b(uv)), texture(TEXTURE, uv_c(uv)), texture(TEXTURE, uv_d(uv)), variant_weights(uv));
	if (has_sibling) {
		vec4 sibling = varied(sibling_texture, uv);
		sibling.rgb *= sibling_tint;
		float n = texture(noise_texture, uv * noise_scale).r;
		float threshold = 1.0 - sibling_cover;
		float weight = smoothstep(threshold - 0.04, threshold + 0.04, n);
		vec4 base = ground;
		ground = mix(base, sibling, weight);
		if (bare_keep < 1.0) {
			float bare = smoothstep(0.0, 0.08, base.r - base.g);
			float kept = patches(uv, 0.06, vec2(0.9, 0.4), bare_keep, 0.1);
			ground = mix(ground, sibling, bare * (1.0 - kept));
		}
	}
	// Dry zones: a soft, wide mask; the tint is applied at most at
	// dry_strength so the change stays a half-tone.
	float d = texture(noise_texture, uv * dry_scale + vec2(0.8, 0.1)).r;
	float dry_threshold = 1.0 - dry_cover;
	float dry = smoothstep(dry_threshold - dry_softness, dry_threshold + dry_softness, d) * dry_strength;
	ground.rgb = mix(ground.rgb, ground.rgb * dry_tint, dry);
	if (has_second) {
		ground = mix(ground, varied(second_texture, uv), patches(uv, second_scale, vec2(0.2, 0.7), second_cover, second_softness));
	}
	if (has_worn) {
		ground = mix(ground, varied(worn_texture, uv), patches(uv, worn_scale, vec2(0.5, 0.25), worn_cover, worn_softness));
	}
	// COLOR comes in already multiplied by the texture: rebuild it from
	// the blended texture and the vertex colour, do not multiply again
	// (that squares the colour and darkens it by half).
	ground.rgb *= base_tint;
	vec3 vertex_tint = tint.rgb;
	// On a ground canvas the vertex colour's green channel carries the
	// nearness of the bank (1 - nearness), never a colour: neutralise it
	// whether or not the shore is drawn.
	vertex_tint.g = 1.0;
	if (current_strength > 0.0) {
		// Two drifts of streaks at different paces, read from the noise
		// squeezed across the river and stretched along it.
		vec2 flow = vec2(uv.x * current_scale, (uv.y - TIME * current_speed) * current_scale / current_stretch);
		vec2 flow2 = vec2(uv.x * current_scale * 1.7 + 0.5, (uv.y - TIME * current_speed * 0.6) * current_scale / current_stretch + 0.3);
		float s1 = texture(noise_texture, flow).r;
		float s2 = texture(noise_texture, flow2).r;
		float t = 1.0 - current_cover;
		float streak = max(smoothstep(t - 0.08, t + 0.08, s1), smoothstep(t, t + 0.12, s2) * 0.6);
		ground.rgb = mix(ground.rgb, ground.rgb * shore_tint, streak * current_strength);
	}
	if (shore_lighten > 0.0) {
		float raw = 1.0 - tint.g;
		float near = raw * shore_lighten;
		float e = texture(noise_texture, uv * edge_scale + vec2(0.61, 0.13)).r;
		near = smoothstep(e - 0.35, e + 0.35, near);
		ground.rgb = mix(ground.rgb, ground.rgb * shore_tint, near);
		if (bank_line > 0.0) {
			float line = smoothstep(0.86, 0.97, raw) * (1.0 - smoothstep(0.985, 1.0, raw));
			ground.rgb *= 1.0 - line * bank_line;
		}
	}
	if (has_field) {
		vec2 world = uv * 4.0;
		float d = (texture(shore_field, world / field_tiles).r * 2.0 - 1.0) * field_range;
		// The noise's features are about 0.2 of its period: read at
		// 0.2 / n per tile, they are about n tiles across.
		float wander = texture(noise_texture, world * (0.2 / field_vary) + vec2(0.31, 0.77)).r;
		float wander2 = texture(noise_texture, world * (0.2 / (field_vary * 0.4)) + vec2(0.62, 0.15)).r;
		float width = mix(field_width_min, field_width_max, smoothstep(0.3, 0.75, wander * 0.75 + wander2 * 0.4));
		// Beaches of sand or pebbles along some stretches.
		float t_beach = 1.0 - field_beach_cover;
		float sand = has_bank2 ? smoothstep(t_beach - 0.03, t_beach + 0.03, texture(noise_texture, world * (0.2 / field_beach) + vec2(0.13, 0.52)).r) : 0.0;
		float pebbles = has_bank3 ? smoothstep(t_beach - 0.03, t_beach + 0.03, texture(noise_texture, world * (0.2 / (field_beach * 0.8)) + vec2(0.71, 0.36)).r) * (1.0 - sand) : 0.0;
		float beach = sand + pebbles;
		width = mix(width, max(width, field_width_max) * field_beach_wide, beach);
		// Tufts: the grass's edge bulges and recedes at two scales.
		float rag = (texture(noise_texture, world * 0.5 + vec2(0.4, 0.9)).r - 0.5) * 2.0 * field_rag
			+ (texture(noise_texture, world * 1.4 + vec2(0.1, 0.3)).r - 0.5) * field_rag;
		float outer = max(width + rag, 0.03);
		if (d > outer + 0.04) {
			discard;
		}
		vec3 colour = ground.rgb;
		if (d < 0.0) {
			float near = 1.0 - smoothstep(0.0, field_light_reach, -d);
			colour = mix(colour, colour * shore_tint, near * field_light);
		} else {
			// The bank's matter, seamless, four metres a texture like the
			// ground: earth, or sand or pebbles on a beach.
			vec2 m_uv = world * 0.25;
			vec3 earth = texture(bank_texture, m_uv).rgb;
			if (beach > 0.0) {
				vec3 other = has_bank2 ? texture(bank2_texture, m_uv + vec2(0.37, 0.11)).rgb : earth;
				if (has_bank3) {
					other = mix(other, texture(bank3_texture, m_uv + vec2(0.61, 0.29)).rgb, pebbles / max(beach, 0.001));
				}
				earth = mix(earth, other, beach);
			}
			earth *= field_earth_tint;
			earth *= mix(field_wet, 1.0, smoothstep(0.0, 0.14, d));
			earth *= mix(1.0, field_shade, smoothstep(outer - 0.14, outer, d));
			colour = earth;
		}
		float line = 1.0 - smoothstep(field_line * 0.6, field_line * 1.4, abs(d));
		// The grass's own outline where it meets the earth.
		float edge = 1.0 - smoothstep(0.0, 0.03, abs(d - outer + 0.015));
		colour = mix(colour, field_ink, max(line, edge * 0.85 * step(0.05, outer)));
		float a = 1.0 - smoothstep(outer, outer + 0.04, d);
		COLOR = vec4(colour * vertex_tint, a);
	} else {
	float alpha = tint.a;
	if (alpha < 1.0 && spill_only_with_bank && !has_bank) {
		alpha = 0.0;
	}
	if (has_bank) {
		// Signed distance from the water line in tiles: positive on the
		// land (the spill's alpha fades over one tile), negative in the
		// water (the shore channel fades over SHORE_WIDTH tiles).
		float s = tint.a < 1.0 ? (1.0 - tint.a) * bank_spill : -(tint.g * 3.0);
		// The bank's extent wanders slowly along the river: how far onto
		// the land, how far into the water.
		// Two wanders of unrelated periods added, so the bank never falls
		// back on one rhythm; the second is read at an odd scale.
		float along = uv.y * bank_vary;
		float w1 = texture(noise_texture, vec2(0.13, along + 0.41)).r;
		float w2 = texture(noise_texture, vec2(0.83, along * 2.7 + 0.11)).r;
		float wander = clamp(w1 * 0.7 + w2 * 0.5 - 0.1, 0.0, 1.0);
		float gate = smoothstep(bank_gate - bank_gate_width, bank_gate + bank_gate_width, wander);
		float reach = mix(bank_reach_min, bank_reach, wander) * gate;
		float i1 = texture(noise_texture, vec2(0.57, along * 0.8 + 0.23)).r;
		float i2 = texture(noise_texture, vec2(0.31, along * 3.1 + 0.67)).r;
		float into = clamp(i1 * 0.8 + i2 * 0.5 - 0.15, 0.0, 1.0) * bank_into * gate;
		float x = clamp((s + into) / max(reach + into, 0.05), 0.0, 1.0);
		vec3 strip = texture(bank_texture, vec2(x, uv.y)).rgb;
		if (has_bank2) {
			float pick = texture(noise_texture, vec2(0.71, along * 0.6 + 0.07)).r;
			strip = mix(strip, texture(bank2_texture, vec2(x, uv.y)).rgb, smoothstep(0.45, 0.55, pick));
		}
		// Both edges fade over a short distance, ragged by a fine grain
		// only, so the bank stays continuous.
		float e1 = texture(noise_texture, uv * bank_edge).r;
		float e2 = texture(noise_texture, uv * bank_edge * 1.3 + vec2(0.5, 0.2)).r;
		float f_out = smoothstep(e1 - 0.35, e1 + 0.35, clamp((reach - s) / 0.3, 0.0, 1.0));
		float f_in = smoothstep(e2 - 0.35, e2 + 0.35, clamp((s + into) / 0.2, 0.0, 1.0));
		float cover = f_out * f_in;
		if (tint.a < 1.0) {
			ground.rgb = strip;
			alpha = cover;
		} else {
			ground.rgb = mix(ground.rgb, strip, cover);
		}
	} else if (alpha < 1.0) {
		float e = texture(noise_texture, uv * edge_scale + vec2(0.23, 0.71)).r;
		alpha = smoothstep(e - 0.3, e + 0.3, alpha);
	}
	COLOR = vec4(ground.rgb * vertex_tint, alpha);
	}
}
