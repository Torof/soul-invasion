// The ground: one seamless top-down texture per terrain, projected onto
// the diamonds (docs/09-visual-production.md). To break its four-metre
// repetition without a second image, the texture is blended with itself
// shifted and turned a quarter turn, by a soft noise: same style and
// colours by construction, no square borders.
shader_type canvas_item;

uniform sampler2D noise_texture : repeat_enable, filter_linear;
// Up to two more compatible textures of the same terrain (made with the
// first as style reference), each shown in soft patches of its own: a
// drier grass over wide areas, a worn grass in a few spots.
// A sister image of the ground (same grass, drawn differently): blended
// with the first over slow noise instead of a turned copy of itself.
uniform sampler2D sibling_texture : repeat_enable, filter_linear;
uniform bool has_sibling = false;
uniform sampler2D second_texture : repeat_enable, filter_linear;
uniform bool has_second = false;
uniform float second_scale = 0.03;
uniform float second_cover = 0.35;
// Tone variation without a second image: the same texture, tinted a
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
uniform sampler2D worn_texture : repeat_enable, filter_linear;
uniform bool has_worn = false;
// Period of the patches, in texture units, and how much ground they take.
uniform float worn_scale = 0.05;
uniform float worn_cover = 0.15;

// Soft patches covering about `cover` of the ground, from the noise read
// at `scale` with an offset so each layer has its own shape.
float patches(vec2 uv, float scale, vec2 shift, float cover) {
	float m = texture(noise_texture, uv * scale + shift).r;
	float threshold = 1.0 - cover;
	return smoothstep(threshold - 0.1, threshold + 0.1, m);
}
// Noise period, in texture units (one unit is one texture, four metres).
uniform float noise_scale = 0.09;
// Where the second copy is taken from, in texture units.
uniform vec2 second_offset = vec2(0.37, 0.63);

// The vertex colour alone: white under a texture, the flat colour of a
// terrain without one, the colour of an item's disc.
varying vec4 tint;

void vertex() {
	tint = COLOR;
}

void fragment() {
	vec2 uv = UV;
	vec4 first = texture(TEXTURE, uv);
	vec2 turned = vec2(-uv.y, uv.x) + second_offset;
	vec4 second = has_sibling ? texture(sibling_texture, uv) : texture(TEXTURE, turned);
	float n = texture(noise_texture, uv * noise_scale).r;
	float weight = smoothstep(0.46, 0.54, n);
	vec4 ground = mix(first, second, weight);
	// Dry zones: a soft, wide mask; the tint is applied at most at
	// dry_strength so the change stays a half-tone.
	float d = texture(noise_texture, uv * dry_scale + vec2(0.8, 0.1)).r;
	float dry_threshold = 1.0 - dry_cover;
	float dry = smoothstep(dry_threshold - dry_softness, dry_threshold + dry_softness, d) * dry_strength;
	ground.rgb = mix(ground.rgb, ground.rgb * dry_tint, dry);
	if (has_second) {
		ground = mix(ground, texture(second_texture, uv), patches(uv, second_scale, vec2(0.2, 0.7), second_cover));
	}
	if (has_worn) {
		ground = mix(ground, texture(worn_texture, uv), patches(uv, worn_scale, vec2(0.5, 0.25), worn_cover));
	}
	// COLOR comes in already multiplied by the texture: rebuild it from
	// the blended texture and the vertex colour, do not multiply again
	// (that squares the colour and darkens it by half).
	ground.rgb *= base_tint;
	COLOR = ground * tint;
}
