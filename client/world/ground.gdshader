// The ground: one seamless top-down texture per terrain, projected onto
// the diamonds (docs/09-visual-production.md). Its four-metre repetition
// is broken by blending it with a sister image over soft noise and, where
// wanted, by laying it in shifted variants chosen by zones of noise. The
// textures are never turned: drawn with a slight three-quarter reading,
// a turned grass reads upside down, like a roof.
//
// Each terrain's canvas covers the rows of tiles it draws in its chunk
// (GroundBuild.rows_of); the ground index (GroundIndex) says, as a safety
// net and for the forest's spill, which tiles this terrain draws. The images
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

// The steps test (13 November 2026, `--world test:marches`): a level per
// tile (HeightIndex: R the height in decimetres, G 255 where known). The
// ground stays flat and is never darkened; where the level drops towards
// the camera (a south or east face), an earth wall is drawn on the lower
// ground below the upper level's lip, STEP_PX_PER_M tall a metre, with a
// short shadow at its foot; on a face turned away, a thin dark line.
// steps_mode: 0 nothing, 1 the walls (A), 2 with a light lip (B), 3 A
// with the stones (C, drawn by StepsTest, not here), 4 all.
uniform sampler2D height_index : filter_nearest, repeat_enable;
uniform int steps_mode = 0;
// A wall's height on the canvas, a metre (Iso.PX_PER_M).
const float STEP_PX_PER_M = 64.0;
// Going up the canvas by a tile along both x and y: a diamond's height.
const float STEP_TILE_PX = 32.0;
// A smaller difference is a ramp: no edge.
const float STEP_MIN_M = 0.95;
const int STEP_MARCH = 16;
// The lip overhangs the wall by LIP_MIN_PX to LIP_MIN_PX + LIP_JITTER_PX,
// ragged along the edge; the foot wanders by FOOT_JITTER_PX either way.
const float LIP_MIN_PX = 2.0;
const float LIP_JITTER_PX = 6.0;
const float FOOT_JITTER_PX = 3.0;
const float FOOT_SHADOW_PX = 6.0;
const float FOOT_SHADOW = 0.3;
const vec3 WALL_EARTH = vec3(0.50, 0.38, 0.26);
const vec3 WALL_DEEP = vec3(0.33, 0.24, 0.16);
// An east face is turned from the light (the screen's upper left).
const float WALL_EAST_SHADE = 0.8;
const float AWAY_LINE_PX = 1.6;
const float AWAY_LINE = 0.45;
const float LIP_LIGHT_PX = 2.5;
const vec3 LIP_LIGHT = vec3(1.28, 1.25, 1.12);

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
// Development: the time the streaks have run for, pinned so that two
// captures can be compared pixel by pixel (SOUL_INVASION_WATER_TIME);
// negative, the clock's own.
uniform float flow_time = -1.0;

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

// A tile's level in metres, or -1 where no chunk said it.
float step_level(vec2 cell) {
	vec4 h = texture(height_index, (cell + 0.5) / index_tiles);
	return h.g > 0.5 ? h.r * 25.5 : -1.0;
}

// `c`, the ground at map point `p` (tiles), with the steps test's walls:
// marches up the canvas (the map's -x and -y at once) across the tiles'
// west and north edges, and stops at the first that rises by a step.
vec3 steps_colour(vec3 c, vec2 p) {
	vec2 a = floor(p);
	float la = step_level(a);
	if (la < 0.0) {
		return c;
	}
	// Distance to the next west and north edges, in tiles along the march.
	vec2 next = p - a;
	for (int i = 0; i < STEP_MARCH; i++) {
		bool west = next.x < next.y;
		float d = west ? next.x : next.y;
		float s = d * STEP_TILE_PX;
		if (s > 3.0 * STEP_PX_PER_M + LIP_MIN_PX + LIP_JITTER_PX + FOOT_JITTER_PX + FOOT_SHADOW_PX) {
			break;
		}
		vec2 b = a - (west ? vec2(1.0, 0.0) : vec2(0.0, 1.0));
		float lb = step_level(b);
		if (lb < 0.0) {
			break;
		}
		// Where along the edge: its own coordinate, for the noise.
		float along = west ? p.y - d : p.x - d;
		if (lb - la >= STEP_MIN_M) {
			float wall = (lb - la) * STEP_PX_PER_M;
			float n1 = texture(noise_texture, vec2(along * 0.9, west ? 0.17 : 0.61)).r;
			float n2 = texture(noise_texture, vec2(along * 3.1, west ? 0.43 : 0.29)).r;
			float lip = LIP_MIN_PX + (n1 * 0.6 + n2 * 0.4) * LIP_JITTER_PX;
			float foot = wall + lip + (texture(noise_texture, vec2(along * 1.7, 0.83)).r - 0.5) * 2.0 * FOOT_JITTER_PX;
			if (s < lip) {
				// The upper ground's lip, overhanging; lit on the faces
				// turned to the light (B).
				bool lit = (steps_mode == 2 || steps_mode == 4) && !west && s > lip - LIP_LIGHT_PX;
				return lit ? c * LIP_LIGHT : c;
			}
			if (s < foot) {
				float v = (s - lip) / max(foot - lip, 1.0);
				// Earth streaked down the wall, deeper under the lip and
				// lighter towards the foot.
				float streak = texture(noise_texture, vec2(along * 6.0, v * 0.15 + (west ? 0.3 : 0.7))).r;
				float clod = texture(noise_texture, vec2(along * 2.3, s / STEP_PX_PER_M * 0.6 + 0.5)).r;
				vec3 earth = mix(WALL_DEEP, WALL_EARTH, smoothstep(0.0, 0.35, v) * 0.8 + streak * 0.35);
				earth *= 0.85 + clod * 0.3;
				return west ? earth * WALL_EAST_SHADE : earth;
			}
			if (s < foot + FOOT_SHADOW_PX) {
				float t = (s - foot) / FOOT_SHADOW_PX;
				return c * (1.0 - FOOT_SHADOW * (1.0 - t) * (1.0 - t));
			}
			return c;
		}
		if (la - lb >= STEP_MIN_M && i == 0) {
			// A face turned away: only its lip, a thin dark line.
			float n = texture(noise_texture, vec2(along * 2.0, 0.37)).r;
			if (s < AWAY_LINE_PX * (0.6 + n * 0.8)) {
				return c * (1.0 - AWAY_LINE);
			}
		}
		if (west) {
			next.x += 1.0;
		} else {
			next.y += 1.0;
		}
		a = b;
		la = lb;
	}
	return c;
}

void fragment() {
	vec2 uv = UV;
	// The clock the streaks run by (pinned by flow_time in development).
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
// The land: its layers, read only where they can show (meet_shows),
// then the terrain's own edge and bank.
vec4 tint = vec4(1.0, 1.0, 1.0, spill);
float alpha = tint.a;
if (alpha < 1.0 && spill_only_with_bank && !has_bank) {
	alpha = 0.0;
}
// The forest's edge over the grass: its own floor, whole or not at all
// along a ragged line (two drawings showing through each other read as
// ghosts), and beyond it a shade fading out, so the line between them
// follows neither the tiles nor a straight edge. Worked out first: where
// the floor does not reach, only the shade shows and no layer is read.
bool edged = !has_bank && alpha < 1.0;
float floor_in = 1.0;
float edge_shade = 0.0;
if (edged) {
	float e = texture(noise_texture, uv * edge_scale + vec2(0.23, 0.71)).r;
	float e2 = texture(noise_texture, uv * edge_scale * 3.7 + vec2(0.61, 0.08)).r;
	float rag = e * 0.65 + e2 * 0.35;
	floor_in = smoothstep(rag - 0.02, rag + 0.02, alpha * 1.15 - 0.15);
	edge_shade = smoothstep(0.0, 1.0, alpha) * fringe_strength;
}
if (edged && floor_in <= 0.0) {
	COLOR = vec4(fringe_colour * tint.rgb, max(floor_in, edge_shade));
} else {
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
	// The base's bare spots (its brown pixels) not kept take the
	// sibling's grass: read only on a bare pixel.
	float bare_taken = 0.0;
	if (bare_keep < 1.0) {
		float bare = smoothstep(0.0, 0.08, ground.r - ground.g);
		if (bare > 0.0) {
			float kept = patches(uv, 0.06, vec2(0.9, 0.4), bare_keep, 0.1);
			bare_taken = bare * (1.0 - kept);
		}
	}
	if (bare_taken > 0.0 || meet_shows(ground, weight, grain)) {
		vec4 sibling = varied_once(sibling_texture, uv, vw, duv_x, duv_y);
		sibling.rgb *= sibling_tint;
		ground = mix(meet(ground, sibling, weight, grain), sibling, bare_taken);
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
} else if (edged) {
	ground.rgb = mix(fringe_colour, ground.rgb, floor_in);
	alpha = max(floor_in, edge_shade);
}
vec3 drawn = ground.rgb * vertex_tint;
if (steps_mode > 0) {
	drawn = steps_colour(drawn, at);
}
COLOR = vec4(drawn, alpha);
}
}
