shader_type canvas_item;
// The bubbles out of focus, drawn once into their own small texture
// (BubbleStack), blurred lightly as it is shown: nine bilinear taps
// weighted 1-2-1 each way, about a 5x5 Gaussian. The texture comes from a
// transparent viewport, its colour already multiplied by its alpha, so it
// is blended premultiplied and the transparent edge stays clean.
render_mode blend_premul_alpha;

// How far apart the taps are, in texture pixels.
uniform float spread = 1.8;

void fragment() {
	vec2 apart = TEXTURE_PIXEL_SIZE * spread;
	vec4 sum = vec4(0.0);
	float total = 0.0;
	for (int x = -1; x <= 1; x++) {
		for (int y = -1; y <= 1; y++) {
			float w = (2.0 - abs(float(x))) * (2.0 - abs(float(y)));
			sum += texture(TEXTURE, UV + vec2(float(x), float(y)) * apart) * w;
			total += w;
		}
	}
	COLOR = sum / total * COLOR.a;
}
