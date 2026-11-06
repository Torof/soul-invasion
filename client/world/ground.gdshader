// The ground: one seamless top-down texture per terrain, projected onto
// the diamonds (docs/09-visual-production.md). To break its four-metre
// repetition without another image, every texture is laid in four
// variants (turned by quarter turns, each shifted) chosen by soft zones
// of noise: same style and colours by construction, no square borders,
// and the period becomes the noise's, far longer than four metres.
shader_type canvas_item;

uniform sampler2D noise_texture : repeat_enable, filter_linear;
// Noise period, in texture units (one unit is one texture, four metres).
uniform float noise_scale = 0.09;
// Period of the zones where one turned variant is laid: wide and few,
// so the ground stays calm; the cuts between them are short.
uniform float variant_scale = 0.05;
// Where the turned copies are taken from, in texture units.
uniform vec2 second_offset = vec2(0.37, 0.63);
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

// Soft patches covering about `cover` of the ground, from the noise read
// at `scale` with an offset so each layer has its own shape.
float patches(vec2 uv, float scale, vec2 shift, float cover) {
	float m = texture(noise_texture, uv * scale + shift).r;
	float threshold = 1.0 - cover;
	return smoothstep(threshold - 0.1, threshold + 0.1, m);
}

// A texture laid in four variants, turned by quarter turns and shifted
// (a seamless texture stays seamless under those), chosen by three soft
// noises, so no four-metre square ever repeats as such. The built-in
// TEXTURE cannot go through a sampler parameter, so the sampling is
// written out where it is used and only the coordinates and weights are
// shared.
vec2 uv_b(vec2 uv) { return vec2(-uv.y, uv.x) + second_offset; }
vec2 uv_c(vec2 uv) { return -uv + vec2(0.11, 0.47); }
vec2 uv_d(vec2 uv) { return vec2(uv.y, -uv.x) + vec2(0.73, 0.29); }
vec3 variant_weights(vec2 uv) {
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
		float n = texture(noise_texture, uv * noise_scale * 0.5 + vec2(0.3, 0.8)).r;
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
	COLOR = ground * tint;
}
