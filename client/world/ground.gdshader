// The ground: one seamless top-down texture per terrain, projected onto
// the diamonds (docs/09-visual-production.md). Its four-metre repetition
// is broken by blending it with a sister image over soft noise and, where
// wanted, by laying it in shifted variants chosen by zones of noise. The
// textures are never turned: drawn with a slight three-quarter reading,
// a turned grass reads upside down, like a roof.
//
// Each terrain's canvas is one quad over its chunk; the ground index
// (GroundIndex) says which of its tiles this terrain draws. The images
// are read with their mipmaps, one level sharper than the hardware would
// choose (SHARPEN): the diamonds squash the texture two to one, and the
// hardware's own level would blur it (D39).
shader_type canvas_item;

// The ground index, a pixel a tile: R the bits of the canvases drawing
// the tile, G the forest's nearness at its top corner (GroundIndex).
uniform sampler2D ground_index : filter_nearest, repeat_enable;
// This terrain's bit, and the tiles the index covers before it repeats.
uniform int index_bit = 1;
uniform float index_tiles = 640.0;
const int INDEX_FOREST = 2;
const int INDEX_FRINGE = 128;
// The images' derivatives are scaled by this: one mipmap level sharper.
const float SHARPEN = 0.5;

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
uniform sampler2D sibling_texture : repeat_enable, filter_linear_mipmap;
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
uniform sampler2D second_texture : repeat_enable, filter_linear_mipmap;
uniform bool has_second = false;
uniform float second_scale = 0.03;
uniform float second_cover = 0.35;
// Width of the fade at a patch's edge (0.1 is a crisp spot; 0.3 or more
// melts one water into another).
uniform float second_softness = 0.1;
uniform sampler2D worn_texture : repeat_enable, filter_linear_mipmap;
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
uniform sampler2D bank_texture : repeat_enable, filter_linear_mipmap;
uniform bool has_bank = false;
// A second strip of another matter (earth beside sand), chosen over
// slow stretches along the river so the bank changes.
uniform sampler2D bank2_texture : repeat_enable, filter_linear_mipmap;
uniform bool has_bank2 = false;
// A third matter (pebbles), not drawn for now.
uniform sampler2D bank3_texture : repeat_enable, filter_linear_mipmap;
uniform bool has_bank3 = false;
// The beds of the fords, seen through their water: up to four matters
// laid side by side in soft patches a few metres across (the brooks' beds).
uniform sampler2D ford1_texture : repeat_enable, filter_linear_mipmap;
uniform sampler2D ford2_texture : repeat_enable, filter_linear_mipmap;
uniform sampler2D ford3_texture : repeat_enable, filter_linear_mipmap;
uniform sampler2D ford4_texture : repeat_enable, filter_linear_mipmap;
uniform bool has_ford1 = false;
uniform bool has_ford2 = false;
uniform bool has_ford3 = false;
uniform bool has_ford4 = false;
// Width of a patch of one bed, in tiles.
uniform float ford_patch = 3.5;
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
uniform sampler2D shore_field : filter_linear, repeat_enable;
// The fords (R) and the current (G, B), a pixel a tile (ShoreField).
uniform sampler2D shore_water : filter_linear, repeat_enable;
// Tiles the field covers before it repeats, and the distance range it
// encodes either way.
uniform float field_tiles = 384.0;
uniform float field_range = 4.0;
// The band of earth: its width in tiles wanders between these over
// stretches of about field_vary tiles; the grass edge is ragged over
// field_rag tiles.
uniform float field_width_min = 0.0;
uniform float field_width_max = 0.55;
uniform float field_vary = 9.0;
uniform float field_rag = 0.16;
// The water's edge: a soft darker line set a little into the water and
// melting into it (half-width and offset in tiles, strength), and the
// ink of the grass's own outline.
uniform float field_line = 0.06;
uniform float field_line_into = 0.04;
uniform float field_line_strength = 0.35;
uniform vec3 field_ink : source_color = vec3(0.17, 0.12, 0.08);
// How far into the water it lightens, in tiles, and how much.
uniform float field_light_reach = 0.9;
uniform float field_light = 0.55;
// The earth's own tone, and how much darker it is where wet and under
// the grass's overhang.
uniform vec3 field_earth_tint : source_color = vec3(1.0);
uniform float field_wet = 0.7;
uniform float field_shade = 0.75;
// Sand beaches (bank2): over stretches of about field_beach tiles, about
// field_beach_cover of the bank, wider there by field_beach_wide. A beach
// runs from one point where the grass meets the water to another. The
// pebbles (bank3) are not drawn for now.
uniform float field_beach = 14.0;
uniform float field_beach_cover = 0.25;
uniform float field_beach_wide = 1.6;
// A beach is at least field_beach_min tiles long; the run is looked for
// up to field_beach_reach tiles either way, at field_beach_samples points.
uniform float field_beach_min = 3.0;
// How close to the beach threshold, in noise units, the bank narrows to
// nothing (where a beach could begin or end).
uniform float field_beach_pinch = 0.02;
uniform float field_beach_reach = 3.0;
const int field_beach_samples = 8;
// How far the grass overhangs the water where it meets it, in tiles, and
// how far the water laps over the earth either way.
uniform float field_overhang = 0.22;
// A ford: its bed of pebbles and sand seen through clear shallow water
// (how much of the water's colour stays over the bed), and the light
// ripples where the current runs over the stones.
uniform float ford_water = 0.42;
// The water thickens from the bank into the ford over this many tiles, so
// the bed shows best at the edge and fades as it deepens.
uniform float ford_shelf = 1.6;
// At a ford the bank gives way: its bed reaches this far over the land
// (tiles), the water's film ford_water_reach beyond the line, and the
// grass fades in over ford_grass_fade.
uniform float ford_bank_reach = 0.6;
uniform float ford_water_reach = 0.35;
uniform float ford_grass_fade = 1.0;
uniform vec3 ford_tint : source_color = vec3(0.88, 1.18, 1.18);
uniform float ford_ripple = 0.35;
uniform float field_lap = 0.07;

// Soft patches covering about `cover` of the ground, from the noise read
// at `scale` with an offset so each layer has its own shape.
float patches(vec2 uv, float scale, vec2 shift, float cover, float softness) {
	float m = texture(noise_texture, uv * scale + shift).r;
	float threshold = 1.0 - cover;
	return smoothstep(threshold - softness, threshold + softness, m);
}

// A texture laid in four variants shifted (never turned), chosen by three
// soft noises, so the four-metre grid is out of phase from one zone to
// the next.
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
// A texture's four variants blended by the weights, reading only the
// variants whose weight is not zero
// (the weights' transition is narrow: nearly everywhere one variant alone
// counts) and with the weights worked out once for all the layers. Read
// with the derivatives of `uv` (dx, dy), so the mipmap level is the one
// the hardware picks for the full blend, inside the branches too.
vec4 varied_once(sampler2D tex, vec2 uv, vec3 w, vec2 dx, vec2 dy) {
	vec4 left = vec4(0.0);
	vec4 right = vec4(0.0);
	if (w.y < 1.0) {
		if (w.x <= 0.0) {
			left = textureGrad(tex, uv, dx, dy);
		} else if (w.x >= 1.0) {
			left = textureGrad(tex, uv_b(uv), dx, dy);
		} else {
			left = mix(textureGrad(tex, uv, dx, dy), textureGrad(tex, uv_b(uv), dx, dy), w.x);
		}
	}
	if (w.y > 0.0) {
		if (w.z <= 0.0) {
			right = textureGrad(tex, uv_c(uv), dx, dy);
		} else if (w.z >= 1.0) {
			right = textureGrad(tex, uv_d(uv), dx, dy);
		} else {
			right = mix(textureGrad(tex, uv_c(uv), dx, dy), textureGrad(tex, uv_d(uv), dx, dy), w.z);
		}
	}
	if (w.y <= 0.0) {
		return left;
	}
	if (w.y >= 1.0) {
		return right;
	}
	return mix(left, right, w.y);
}
// The same for the canvas's own texture (TEXTURE): a function may take
// built-in textures or uniform ones, never both.
vec4 varied_once_builtin(sampler2D tex, vec2 uv, vec3 w, vec2 dx, vec2 dy) {
	vec4 left = vec4(0.0);
	vec4 right = vec4(0.0);
	if (w.y < 1.0) {
		if (w.x <= 0.0) {
			left = textureGrad(tex, uv, dx, dy);
		} else if (w.x >= 1.0) {
			left = textureGrad(tex, uv_b(uv), dx, dy);
		} else {
			left = mix(textureGrad(tex, uv, dx, dy), textureGrad(tex, uv_b(uv), dx, dy), w.x);
		}
	}
	if (w.y > 0.0) {
		if (w.z <= 0.0) {
			right = textureGrad(tex, uv_c(uv), dx, dy);
		} else if (w.z >= 1.0) {
			right = textureGrad(tex, uv_d(uv), dx, dy);
		} else {
			right = mix(textureGrad(tex, uv_c(uv), dx, dy), textureGrad(tex, uv_d(uv), dx, dy), w.z);
		}
	}
	if (w.y <= 0.0) {
		return left;
	}
	if (w.y >= 1.0) {
		return right;
	}
	return mix(left, right, w.y);
}

// The water flowing: two layers of the water's texture, each advected
// along the current in two phases half a period apart and blended so the
// reset never shows; the second layer turned and smaller, so neither
// layer's pattern repeats where the eye looks.
uniform float flow_period = 2.4;
uniform float flow_mix = 0.45;
// How much darker and cooler the water is in the middle of the river,
// reached at deep_reach tiles from the bank.
uniform vec3 deep_tint : source_color = vec3(0.72, 0.86, 0.92);
uniform float deep_reach = 2.5;
// Glints: small bright sparkles drifting with the water.
uniform float glint = 0.35;

// Development: the time the water has run for, pinned so that two
// captures can be compared pixel by pixel (SOUL_INVASION_WATER_TIME);
// negative, the clock's own.
uniform float flow_time = -1.0;

vec3 flowing(sampler2D tex, vec2 world, vec2 flow, float period, float scale, mat2 turn, float shift, float now, vec2 dx, vec2 dy) {
	float t = now / period + shift;
	float p0 = fract(t);
	float p1 = fract(t + 0.5);
	float w0 = 1.0 - abs(1.0 - 2.0 * p0);
	vec2 base = turn * world * (0.25 * scale);
	vec2 f = turn * flow * (period * 0.25 * scale);
	vec2 gx = turn * dx * scale;
	vec2 gy = turn * dy * scale;
	vec3 a = textureGrad(tex, base - f * p0 + vec2(shift, 0.0), gx, gy).rgb;
	vec3 b = textureGrad(tex, base - f * p1 + vec2(0.0, shift) + vec2(0.5), gx, gy).rgb;
	return a * w0 + b * (1.0 - w0);
}

// The bed of the fords at a point of the map in tiles: up to four matters
// laid side by side in soft patches.
vec3 ford_bed(vec2 world, vec2 dx, vec2 dy) {
	vec2 b_uv = world * 0.25;
	float f = 0.2 / ford_patch;
	vec4 n = vec4(
		texture(noise_texture, world * f + vec2(0.11, 0.73)).r,
		has_ford2 ? texture(noise_texture, world * f + vec2(0.57, 0.19)).r : 0.0,
		has_ford3 ? texture(noise_texture, world * f + vec2(0.83, 0.41)).r : 0.0,
		has_ford4 ? texture(noise_texture, world * f + vec2(0.29, 0.95)).r : 0.0);
	vec4 w = pow(max(n, vec4(0.0)), vec4(8.0));
	w /= max(w.x + w.y + w.z + w.w, 1e-5);
	vec3 bed = textureGrad(ford1_texture, b_uv, dx, dy).rgb * w.x;
	if (has_ford2) { bed += textureGrad(ford2_texture, b_uv + vec2(0.31, 0.17), dx, dy).rgb * w.y; }
	if (has_ford3) { bed += textureGrad(ford3_texture, b_uv + vec2(0.67, 0.43), dx, dy).rgb * w.z; }
	if (has_ford4) { bed += textureGrad(ford4_texture, b_uv + vec2(0.13, 0.89), dx, dy).rgb * w.w; }
	return bed;
}

// Two grounds meet without a ghost: rather than fading one drawing over
// the other (two half-transparent drawings read as doubled tufts), the
// one whose drawing is darker there (a tuft, its ink) wins, within a
// narrow band around where the mask changes. `w` is the soft mask.
uniform float meet_band = 0.12;
uniform float meet_depth = 0.3;
// Where the mask changes, the two grounds interlock in pieces about a
// tile across (each piece whole), instead of fading over each other.
uniform float meet_grain = 0.9;
// The shade a spilling ground lays over its neighbour, and how dark.
uniform vec3 fringe_colour : source_color = vec3(0.237, 0.402, 0.181);
uniform float fringe_strength = 0.55;
vec4 meet(vec4 a, vec4 b, float w, float meet_noise) {
	float da = 1.0 - dot(a.rgb, vec3(0.333));
	float db = 1.0 - dot(b.rgb, vec3(0.333));
	float t = (w - 0.5) * 2.0 + (db - da) * meet_depth + meet_noise;
	return mix(a, b, smoothstep(-meet_band, meet_band, t));
}
// Whether `meet(a, b, w, meet_noise)` can show anything of b, whatever b
// is (its darkness is at most 1): when it cannot, the result is a,
// exactly, and b need not be read.
bool meet_shows(vec4 a, float w, float meet_noise) {
	return (w - 0.5) * 2.0 + dot(a.rgb, vec3(0.333)) * meet_depth + meet_noise > -meet_band;
}

// The beach noise at sample `k` of the window along the bank: the
// samples are field_beach_samples either way from the point of the
// water's line nearest the pixel, `spacing` tiles apart.
float beach_noise(vec2 on_line, vec2 along_bank, int k, float spacing) {
	vec2 at = on_line + along_bank * float(k - field_beach_samples) * spacing;
	return texture(noise_texture, at * (0.2 / field_beach) + vec2(0.13, 0.52)).r;
}

void fragment() {
	vec2 uv = UV;
	// The clock the water runs by (pinned by flow_time in development).
	float now = flow_time < 0.0 ? TIME : flow_time;
	// The derivatives the images are read with, before a pixel can be
	// discarded, a mipmap level sharper.
	vec2 duv_x = dFdx(uv) * SHARPEN;
	vec2 duv_y = dFdy(uv) * SHARPEN;
	// This terrain's tiles, from the ground index; the forest's spill
	// over its neighbours, its opacity spread from the tile's corners over
	// the diamond's two triangles, as a mesh's vertex colours were.
	vec2 at = uv * 4.0;
	vec2 cell = floor(at);
	vec2 in_cell = at - cell;
	vec4 index = texture(ground_index, (cell + 0.5) / index_tiles);
	int bits = int(round(index.r * 255.0));
	float spill = 1.0;
	if ((bits & index_bit) == 0) {
		if (index_bit != INDEX_FOREST || (bits & INDEX_FRINGE) == 0) {
			discard;
		}
		float v0 = index.g;
		float v1 = texture(ground_index, (cell + vec2(1.5, 0.5)) / index_tiles).g;
		float v2 = texture(ground_index, (cell + vec2(1.5, 1.5)) / index_tiles).g;
		float v3 = texture(ground_index, (cell + vec2(0.5, 1.5)) / index_tiles).g;
		spill = in_cell.x >= in_cell.y
			? v0 + in_cell.x * (v1 - v0) + in_cell.y * (v2 - v1)
			: v0 + in_cell.x * (v2 - v3) + in_cell.y * (v3 - v0);
	}
	if (has_field) {
		// The water canvas (D36): the water, its bank and the land within
		// two tiles of it. The picture is settled; the work is laid out so
		// that a pixel pays only for what can show at its distance from
		// the water's line, with the same result to the bit where a layer
		// contributes (D39, 11 November 2026). The bounds below are
		// arithmetic on the uniforms alone.
		vec2 world = uv * 4.0;
		vec2 fuv = world / field_tiles;
		float d = (texture(shore_field, fuv).r * 2.0 - 1.0) * field_range;
		// Farthest the grass's edge can lie from the water's line: the
		// widest bank (the earth, a beach), the tufts' bulge, a ford's bed.
		float rag_max = 1.5 * abs(field_rag);
		float width_top = max(max(field_width_min, field_width_max), 0.0);
		width_top = max(width_top, width_top * field_beach_wide);
		float outer_top = width_top + rag_max;
		float outer_top_ford = max(outer_top, ford_bank_reach + rag_max);
		// A hair past the bound, so a rounding of the exact test below can
		// never keep a pixel this bound would have sent away.
		const float SLACK = 0.01;
		// Past the farthest edge there can be, with or without a ford: land
		// the canvas does not draw. One read, instead of the whole bank.
		if (d > max(outer_top_ford, 0.0) + max(0.04, ford_grass_fade) + SLACK) {
			discard;
		}
		// The fords (R) and the current (G, B), read once for all.
		vec4 field_px = texture(shore_water, fuv);
		float ford_bank = has_ford1 ? smoothstep(0.03, 0.4, field_px.r) : 0.0;
		// Over a ford's bank the grass fades in over this width instead of
		// stopping on a line.
		float grass_fade = mix(0.04, ford_grass_fade, ford_bank);
		// The same bound, knowing whether a ford widens the bank here.
		float outer_bound = ford_bank > 0.0 ? outer_top_ford : outer_top;
		if (d > max(outer_bound, 0.0) + grass_fade + SLACK) {
			discard;
		}
		// Out in the water, past where the bank's edge, its line, its
		// light and a ford's shelf can still be felt, the water's colour
		// depends on nothing the bank computes: every term fed by dw or by
		// the grass's edge is saturated there, whatever the lap, the tufts
		// and the beaches would have said. The lap moves the water's edge
		// by at most lap_max; the ford's film by ford_water_reach.
		float lap_max = 1.5 * abs(field_lap) + max(0.0, -ford_water_reach);
		// Lowest the grass's edge can lie (the tufts' overhang, a ford's
		// bed cut by the rag).
		float grass_min = min(-max(field_overhang, 0.0), min(0.0, ford_bank_reach - rag_max));
		// Past this depth the grass's own outline and its fade are whole
		// water, so the tufts need not be read.
		float edge_limit = -(max(0.045, abs(grass_fade)) - grass_min) - SLACK;
		float deep_limit = -(max(max(deep_reach, field_light_reach), max(ford_shelf, field_line + field_line_into)) + lap_max) - SLACK;
		deep_limit = min(deep_limit, min(edge_limit, -lap_max - SLACK));
		float dw;
		float grass_at;
		float outer;
		float beach;
		if (d <= deep_limit) {
			// Deep water: the distance itself stands for the water's edge
			// (every use of it below is saturated at this depth).
			dw = d;
			grass_at = 0.0;
			outer = 0.0;
			beach = 0.0;
		} else {
			// Along the bank: perpendicular to the field's slope.
			float step_f = 0.5 / field_tiles;
			vec2 grad = vec2(
				texture(shore_field, fuv + vec2(step_f, 0.0)).r - texture(shore_field, fuv - vec2(step_f, 0.0)).r,
				texture(shore_field, fuv + vec2(0.0, step_f)).r - texture(shore_field, fuv - vec2(0.0, step_f)).r);
			vec2 across = length(grad) > 1e-5 ? normalize(grad) : vec2(0.0, 1.0);
			vec2 along_bank = vec2(-across.y, across.x);
			// The point of the water's line nearest to here: what varies along
			// the bank (its width, its beaches) is read there, so it is the
			// same right across the bank.
			vec2 on_line = world - across * d;
			// The noise's features are about 0.2 of its period: read at
			// 0.2 / n per tile, they are about n tiles across.
			float wander = texture(noise_texture, on_line * (0.2 / field_vary) + vec2(0.31, 0.77)).r;
			float wander2 = texture(noise_texture, on_line * (0.2 / (field_vary * 0.4)) + vec2(0.62, 0.15)).r;
			float width = mix(field_width_min, field_width_max, smoothstep(0.3, 0.75, wander * 0.75 + wander2 * 0.4));
			// Sand beaches: where the beach noise is high over a run of at
			// least field_beach_min tiles along the bank. The noise is read at
			// field_beach_samples points either way; the run around this point
			// tells how far the beach goes each way, and the grass meets the
			// water at its two ends.
			float t_beach = 1.0 - field_beach_cover;
			float sand = 0.0;
			float pinch = 0.0;
			if (has_bank2) {
				float spacing = field_beach_reach / float(field_beach_samples);
				float far = float(field_beach_samples) * spacing;
				float to_end = -1.0;
				float to_beach = 1e3;
				float centre = beach_noise(on_line, along_bank, field_beach_samples, spacing);
				if (field_beach_min >= field_beach_reach && centre != t_beach) {
					// A run that does not pass through this point lies on one
					// side of it, so it is shorter than the reach: when a beach
					// must be at least as long as the reach, only the run through
					// this point can be one. It is followed from here outwards,
					// sample by sample, as far as it goes: one read where the
					// noise is low here (most of the bank), the same values as
					// the whole window where it is high. (The noise exactly at
					// the threshold is the one case where a run ending here
					// counts as passing through: that case reads the window.)
					if (centre > t_beach) {
						int k = field_beach_samples;
						float at_k = centre;
						float below_k = 0.0;
						while (k > 0) {
							float n = beach_noise(on_line, along_bank, k - 1, spacing);
							if (n <= t_beach) {
								below_k = n;
								break;
							}
							at_k = n;
							k--;
						}
						int j = field_beach_samples;
						float at_j = centre;
						float below_j = 0.0;
						while (j < 2 * field_beach_samples) {
							float n = beach_noise(on_line, along_bank, j + 1, spacing);
							if (n <= t_beach) {
								below_j = n;
								break;
							}
							at_j = n;
							j++;
						}
						float start = -far;
						if (k > 0) {
							start = (float(k - 1) + (t_beach - below_k) / max(at_k - below_k, 1e-4) - float(field_beach_samples)) * spacing;
						}
						float end = far;
						if (j < 2 * field_beach_samples) {
							end = (float(j) + (t_beach - at_j) / min(below_j - at_j, -1e-4) - float(field_beach_samples)) * spacing;
						}
						if (end - start >= field_beach_min) {
							to_end = min(-start, end);
						}
					}
				} else {
					// The beach noise along the bank, either way from here.
					float nb[2 * field_beach_samples + 1];
					for (int k = 0; k <= 2 * field_beach_samples; k++) {
						nb[k] = beach_noise(on_line, along_bank, k, spacing);
					}
					// Each run above the threshold is a beach if it is long enough;
					// its ends are found between samples, so they move smoothly.
					int k = 0;
					while (k <= 2 * field_beach_samples) {
						if (nb[k] <= t_beach) {
							k++;
							continue;
						}
						int j = k;
						while (j < 2 * field_beach_samples && nb[j + 1] > t_beach) {
							j++;
						}
						float start = -far;
						if (k > 0) {
							start = (float(k - 1) + (t_beach - nb[k - 1]) / max(nb[k] - nb[k - 1], 1e-4) - float(field_beach_samples)) * spacing;
						}
						float end = far;
						if (j < 2 * field_beach_samples) {
							end = (float(j) + (t_beach - nb[j]) / min(nb[j + 1] - nb[j], -1e-4) - float(field_beach_samples)) * spacing;
						}
						if (end - start >= field_beach_min) {
							if (start <= 0.0 && end >= 0.0) {
								to_end = min(-start, end);
							} else {
								to_beach = min(to_beach, start > 0.0 ? start : -end);
							}
						}
						k = j + 1;
					}
				}
				if (to_end >= 0.0) {
					sand = smoothstep(0.25, 0.8, to_end);
					pinch = 1.0 - smoothstep(0.0, 0.4, to_end);
				} else {
					pinch = 1.0 - smoothstep(0.0, 0.4, to_beach);
				}
			}
			beach = sand;
			width = mix(width, max(width, field_width_max) * field_beach_wide, beach);
			width *= 1.0 - pinch;
			// Tufts: the grass's edge bulges and recedes at two scales.
			float rag = (texture(noise_texture, world * 0.5 + vec2(0.4, 0.9)).r - 0.5) * 2.0 * field_rag
				+ (texture(noise_texture, world * 1.4 + vec2(0.1, 0.3)).r - 0.5) * field_rag;
			outer = max(width + rag * clamp(width / 0.12, 0.0, 1.0), 0.0);
			// At a ford there is no bank: its bed runs on over the land, the
			// water reaches further in a thin film, and the grass comes back
			// gradually over the bed.
			outer = mix(outer, ford_bank_reach + rag, ford_bank);
			// Where the bank narrows to nothing, the grass overhangs the water
			// in tufts instead of stopping on the line. Out past edge_limit
			// the overhang changes nothing (the outline and the fade are
			// saturated there), so the tufts are not read.
			float overhang = 0.0;
			if (d > edge_limit) {
				float tufts = texture(noise_texture, world * 1.9 + vec2(0.83, 0.47)).r * 0.7
					+ texture(noise_texture, world * 4.3 + vec2(0.27, 0.66)).r * 0.5;
				overhang = field_overhang * clamp(tufts - 0.3, 0.0, 1.0);
			}
			grass_at = mix(-overhang, outer, smoothstep(0.03, 0.14, outer));
			grass_at = mix(grass_at, outer, ford_bank);
			if (d > grass_at + grass_fade) {
				discard;
			}
			// The water's own edge wanders over the earth: the water laps it.
			float lap = (texture(noise_texture, world * 1.2 + vec2(0.55, 0.05)).r - 0.5) * 2.0 * field_lap
				+ (texture(noise_texture, world * 3.7 + vec2(0.18, 0.92)).r - 0.5) * field_lap;
			dw = d - lap * smoothstep(0.02, 0.15, outer) - ford_water_reach * ford_bank;
		}
		vec3 colour;
		if (dw < 0.0) {
			vec2 flow = (field_px.gb * 2.0 - 1.0) * 2.0;
			// Tiles a second, a little slower than the water itself reads.
			vec2 runs = flow * 0.8;
			vec3 w1 = flowing(TEXTURE, world, runs, flow_period, 1.0, mat2(vec2(1.0, 0.0), vec2(0.0, 1.0)), 0.0, now, duv_x, duv_y);
			vec3 w2 = flowing(TEXTURE, world, runs * 1.15, flow_period * 1.3, 1.6, mat2(vec2(0.0, 1.0), vec2(-1.0, 0.0)), 0.37, now, duv_x, duv_y);
			colour = mix(w1, w2, flow_mix) * base_tint;
			// Deeper in the middle.
			colour *= mix(vec3(1.0), deep_tint, smoothstep(0.3, deep_reach, -dw));
			// Glints drifting with the water.
			float g1 = texture(noise_texture, world * 1.7 - runs * now * 0.4).r;
			float g2 = texture(noise_texture, world * 2.9 + vec2(0.3, 0.6) - runs * now * 0.55).r;
			colour = mix(colour, vec3(1.0), smoothstep(0.82, 0.9, g1 * 0.55 + g2 * 0.55) * glint);
			float near = 1.0 - smoothstep(0.0, field_light_reach, -dw);
			colour = mix(colour, colour * shore_tint, near * field_light);
			// Into the deep water: the bed fades out slowly.
			float ford = smoothstep(0.02, 0.97, field_px.r);
			ford = ford * ford * (3.0 - 2.0 * ford);
			// Near a ford's bank the water is all ford, so its film meets the
			// bed on the land without a line.
			ford = max(ford, ford_bank * (1.0 - smoothstep(0.0, ford_shelf, -dw)));
			if (ford > 0.0 && has_ford1) {
				vec3 bed = ford_bed(world, duv_x, duv_y);
				// From the bank inwards, the water over the bed thickens.
				// At the edge of a ford's bank the water is a film that
				// thins to nothing on the bed, so there is no line.
				float shelf = smoothstep(0.0, ford_shelf, -dw);
				float deep = mix(mix(0.18, 0.0, ford_bank), ford_water, shelf);
				vec3 wet_bed = bed * mix(vec3(0.85), ford_tint * 1.35, mix(1.0, shelf, ford_bank));
				vec3 shallow = mix(wet_bed, colour, deep);
				// Ripples drifting with the current over the stones.
				float r1 = texture(noise_texture, world * 0.9 + vec2(now * 0.03, now * 0.05)).r;
				float r2 = texture(noise_texture, world * 2.3 - vec2(now * 0.04, now * 0.02)).r;
				float ripple = smoothstep(0.62, 0.72, r1 * 0.6 + r2 * 0.5);
				shallow = mix(shallow, shallow * shore_tint * 1.15, ripple * ford_ripple);
				colour = mix(colour, shallow, ford);
			}
		} else {
			// The bank's matter, seamless, four metres a texture like the
			// ground: earth, or sand on a beach.
			vec2 m_uv = world * 0.25;
			vec3 earth = textureGrad(bank_texture, m_uv, duv_x, duv_y).rgb;
			if (beach > 0.0) {
				earth = mix(earth, textureGrad(bank2_texture, m_uv + vec2(0.37, 0.11), duv_x, duv_y).rgb, beach);
			}
			earth *= field_earth_tint;
			earth *= mix(field_wet, 1.0, smoothstep(0.0, 0.14, dw));
			earth *= mix(1.0, field_shade, smoothstep(outer - 0.14, outer, d));
			if (ford_bank > 0.0) {
				// The ford's bed, damp, wetter near the water.
				vec3 bed = ford_bed(world, duv_x, duv_y) * mix(0.8, 1.0, smoothstep(0.0, 0.5, dw));
				earth = mix(earth, bed, ford_bank);
			}
			colour = earth;
		}
		// The water's edge: darker, melting into the water.
		float line = (1.0 - smoothstep(0.0, field_line, abs(dw + field_line_into))) * field_line_strength * (1.0 - ford_bank);
		colour = mix(colour, colour * 0.45, line);
		// The grass's own outline, against the earth or the water.
		float edge = 1.0 - smoothstep(0.0, 0.03, abs(d - grass_at + 0.015));
		colour = mix(colour, field_ink, edge * 0.85 * (1.0 - ford_bank));
		// The grass's edge over a ford's bank: ragged by a fine noise as it
		// fades, so it comes back in tufts. Away from a ford it is nothing,
		// exactly, and need not be read.
		float tuft_in = 0.0;
		if (ford_bank > 0.0) {
			tuft_in = ((texture(noise_texture, world * 3.1 + vec2(0.37, 0.58)).r - 0.5) * 1.2
				+ (texture(noise_texture, world * 1.1 + vec2(0.71, 0.12)).r - 0.5) * 0.8) * grass_fade * ford_bank;
		}
		float a = 1.0 - smoothstep(grass_at + tuft_in, grass_at + tuft_in + grass_fade, d);
		COLOR = vec4(colour, a);
	} else {
	// The land: its layers, read only where they can show (meet_shows),
	// then the terrain's own edge and bank.
	vec4 tint = vec4(1.0, 1.0, 1.0, spill);
	float grain = (texture(noise_texture, uv * 0.8 + vec2(0.37, 0.19)).r - 0.5) * 2.0 * meet_grain;
	// The variants' weights, once for every layer, and the derivatives the
	// layers are read with (varied_once).
	vec3 vw = variant_weights(uv);
	vec4 ground = varied_once_builtin(TEXTURE, uv, vw, duv_x, duv_y);
	// Each layer is read only where it can show (meet_shows): the same
	// picture, fewer reads.
	if (has_sibling) {
		float n = texture(noise_texture, uv * noise_scale).r;
		float threshold = 1.0 - sibling_cover;
		float weight = smoothstep(threshold - 0.15, threshold + 0.15, n);
		if (bare_keep < 1.0 || meet_shows(ground, weight, grain)) {
			vec4 sibling = varied_once(sibling_texture, uv, vw, duv_x, duv_y);
			sibling.rgb *= sibling_tint;
			vec4 base = ground;
			ground = meet(base, sibling, weight, grain);
			if (bare_keep < 1.0) {
				float bare = smoothstep(0.0, 0.08, base.r - base.g);
				float kept = patches(uv, 0.06, vec2(0.9, 0.4), bare_keep, 0.1);
				ground = mix(ground, sibling, bare * (1.0 - kept));
			}
		}
	}
	// Dry zones: a soft, wide mask; the tint is applied at most at
	// dry_strength so the change stays a half-tone.
	float d = texture(noise_texture, uv * dry_scale + vec2(0.8, 0.1)).r;
	float dry_threshold = 1.0 - dry_cover;
	float dry = smoothstep(dry_threshold - dry_softness, dry_threshold + dry_softness, d) * dry_strength;
	ground.rgb = mix(ground.rgb, ground.rgb * dry_tint, dry);
	if (has_second) {
		float w2 = patches(uv, second_scale, vec2(0.2, 0.7), second_cover, second_softness);
		if (meet_shows(ground, w2, grain)) {
			ground = meet(ground, varied_once(second_texture, uv, vw, duv_x, duv_y), w2, grain);
		}
	}
	if (has_worn) {
		float w3 = patches(uv, worn_scale, vec2(0.5, 0.25), worn_cover, worn_softness);
		if (meet_shows(ground, w3, grain)) {
			ground = meet(ground, varied_once(worn_texture, uv, vw, duv_x, duv_y), w3, grain);
		}
	}
	// COLOR comes in already multiplied by the texture: rebuild it from
	// the blended texture and the vertex colour, do not multiply again
	// (that squares the colour and darkens it by half).
	ground.rgb *= base_tint;
	vec3 vertex_tint = tint.rgb;
	if (current_strength > 0.0) {
		// Two drifts of streaks at different paces, read from the noise
		// squeezed across the river and stretched along it.
		vec2 flow = vec2(uv.x * current_scale, (uv.y - now * current_speed) * current_scale / current_stretch);
		vec2 flow2 = vec2(uv.x * current_scale * 1.7 + 0.5, (uv.y - now * current_speed * 0.6) * current_scale / current_stretch + 0.3);
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
		vec3 strip = textureGrad(bank_texture, vec2(x, uv.y), vec2(0.0), vec2(0.0)).rgb;
		if (has_bank2) {
			float pick = texture(noise_texture, vec2(0.71, along * 0.6 + 0.07)).r;
			strip = mix(strip, textureGrad(bank2_texture, vec2(x, uv.y), vec2(0.0), vec2(0.0)).rgb, smoothstep(0.45, 0.55, pick));
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
		// The forest's edge over the grass: its own floor, whole or not at
		// all along a ragged line (two drawings showing through each other
		// read as ghosts), and beyond it a shade fading out, so the line
		// between them follows neither the tiles nor a straight edge.
		float e2 = texture(noise_texture, uv * edge_scale * 3.7 + vec2(0.61, 0.08)).r;
		float rag = e * 0.65 + e2 * 0.35;
		float floor_in = smoothstep(rag - 0.02, rag + 0.02, alpha * 1.15 - 0.15);
		float shade = smoothstep(0.0, 1.0, alpha) * fringe_strength;
		ground.rgb = mix(fringe_colour, ground.rgb, floor_in);
		alpha = max(floor_in, shade);
	}
	COLOR = vec4(ground.rgb * vertex_tint, alpha);
	}
}
