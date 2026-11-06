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
// Share of the ground the sibling covers.
uniform float sibling_cover = 0.5;
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
uniform sampler2D worn_texture : repeat_enable, filter_linear;
uniform bool has_worn = false;
// Period of the patches, in texture units, and how much ground they take.
uniform float worn_scale = 0.05;
uniform float worn_cover = 0.15;
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

// Soft patches covering about `cover` of the ground, from the noise read
// at `scale` with an offset so each layer has its own shape.
float patches(vec2 uv, float scale, vec2 shift, float cover) {
	float m = texture(noise_texture, uv * scale + shift).r;
	float threshold = 1.0 - cover;
	return smoothstep(threshold - 0.1, threshold + 0.1, m);
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
		float n = texture(noise_texture, uv * noise_scale).r;
		float threshold = 1.0 - sibling_cover;
		float weight = smoothstep(threshold - 0.04, threshold + 0.04, n);
		vec4 base = ground;
		ground = mix(base, sibling, weight);
		if (bare_keep < 1.0) {
			float bare = smoothstep(0.0, 0.08, base.r - base.g);
			float kept = patches(uv, 0.06, vec2(0.9, 0.4), bare_keep);
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
		ground = mix(ground, varied(second_texture, uv), patches(uv, second_scale, vec2(0.2, 0.7), second_cover));
	}
	if (has_worn) {
		ground = mix(ground, varied(worn_texture, uv), patches(uv, worn_scale, vec2(0.5, 0.25), worn_cover));
	}
	// COLOR comes in already multiplied by the texture: rebuild it from
	// the blended texture and the vertex colour, do not multiply again
	// (that squares the colour and darkens it by half).
	ground.rgb *= base_tint;
	vec3 vertex_tint = tint.rgb;
	if (shore_lighten > 0.0) {
		float near = (1.0 - tint.g) * shore_lighten;
		float e = texture(noise_texture, uv * edge_scale + vec2(0.61, 0.13)).r;
		near = smoothstep(e - 0.35, e + 0.35, near);
		ground.rgb = mix(ground.rgb, ground.rgb * shore_tint, near);
		vertex_tint.g = 1.0;
	}
	float alpha = tint.a;
	if (alpha < 1.0) {
		float e = texture(noise_texture, uv * edge_scale + vec2(0.23, 0.71)).r;
		alpha = smoothstep(e - 0.3, e + 0.3, alpha);
	}
	COLOR = vec4(ground.rgb * vertex_tint, alpha);
}
