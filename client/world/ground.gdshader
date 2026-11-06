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
uniform sampler2D second_texture : repeat_enable, filter_linear;
uniform bool has_second = false;
uniform float second_scale = 0.03;
uniform float second_cover = 0.45;
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
	vec4 second = texture(TEXTURE, turned);
	float n = texture(noise_texture, uv * noise_scale).r;
	float weight = smoothstep(0.46, 0.54, n);
	vec4 ground = mix(first, second, weight);
	if (has_second) {
		ground = mix(ground, texture(second_texture, uv), patches(uv, second_scale, vec2(0.2, 0.7), second_cover));
	}
	if (has_worn) {
		ground = mix(ground, texture(worn_texture, uv), patches(uv, worn_scale, vec2(0.5, 0.25), worn_cover));
	}
	// COLOR comes in already multiplied by the texture: rebuild it from
	// the blended texture and the vertex colour, do not multiply again
	// (that squares the colour and darkens it by half).
	COLOR = ground * tint;
}
