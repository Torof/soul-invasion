// The ground: one seamless top-down texture per terrain, projected onto
// the diamonds (docs/09-visual-production.md). To break its four-metre
// repetition without a second image, the texture is blended with itself
// shifted and turned a quarter turn, by a soft noise: same style and
// colours by construction, no square borders.
shader_type canvas_item;

uniform sampler2D noise_texture : repeat_enable, filter_linear;
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
	// COLOR comes in already multiplied by the texture: rebuild it from
	// the blended texture and the vertex colour, do not multiply again
	// (that squares the colour and darkens it by half).
	COLOR = mix(first, second, weight) * tint;
}
