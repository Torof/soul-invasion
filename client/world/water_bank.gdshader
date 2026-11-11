// The bank (WaterMeshes): the band along the water's line, water on one
// side and the bank's earth on the other, the grass beneath showing past
// a ragged edge. The vertices carry the distance to the line (UV.x,
// tiles, positive on the land), the earth's width there after its wander
// and its beaches (UV.y), the current, the sand and the ford (COLOR).
// The pixel reads one detail texel (rag, tufts, lap, ripples), then the
// water (two reads) on the water side or the earth (one, two on a beach)
// on the land side; a ford adds its bed. The ink lines come from the
// distance itself, with its screen derivative, so they are crisp at every
// zoom and never thinner than a pixel and a half.
shader_type canvas_item;

#include "water_common.gdshaderinc"

uniform sampler2D earth_texture : repeat_enable, filter_linear_mipmap;
uniform sampler2D sand_texture : repeat_enable, filter_linear_mipmap;
uniform bool has_sand = false;
// The grass edge: ragged over field_rag tiles; where the bank narrows to
// nothing the grass overhangs the water in tufts by field_overhang.
uniform float field_rag = 0.16;
uniform float field_overhang = 0.22;
// The water's edge: a soft darker line set a little into the water
// (half-width and offset in tiles, strength), the water lapping over the
// earth by field_lap, and the ink of the grass's own outline.
uniform float field_line = 0.06;
uniform float field_line_into = 0.04;
uniform float field_line_strength = 0.35;
uniform float field_lap = 0.07;
uniform vec3 field_ink : source_color = vec3(0.17, 0.12, 0.08);
// The earth's own tone, and how much darker it is where wet and under the
// grass's overhang.
uniform vec3 field_earth_tint : source_color = vec3(1.0);
uniform float field_wet = 0.7;
uniform float field_shade = 0.75;
// At a ford the bank gives way: its bed reaches this far over the land,
// the water's film ford_water_reach beyond the line, and the grass fades
// in over ford_grass_fade.
uniform float ford_bank_reach = 0.6;
uniform float ford_water_reach = 0.35;
uniform float ford_grass_fade = 1.0;

varying vec4 tile;
varying vec2 edge;
varying vec2 world;

void vertex() {
	tile = COLOR;
	edge = UV;
	// The point of the map in tiles, from the vertex's place on the screen
	// (the inverse of WaterMeshes.to_screen).
	vec2 px = (MODEL_MATRIX * vec4(VERTEX, 0.0, 1.0)).xy;
	world = vec2(px.x / 64.0 + (px.y + 16.0) / 32.0, (px.y + 16.0) / 32.0 - px.x / 64.0);
}

void fragment() {
	if (split_edge >= 0.0 && SCREEN_UV.x < split_edge) {
		discard;
	}
	float d = edge.x;
	float width = edge.y;
	float now = flow_time < 0.0 ? TIME : flow_time;
	float ford_bank = has_bed ? smoothstep(0.03, 0.4, tile.a) : 0.0;
	// Over a ford's bank the grass fades in over this width instead of
	// stopping on a line.
	float grass_fade = mix(0.04, ford_grass_fade, ford_bank);
	// The detail: the rag, the tufts, the lap; far out, none of them.
	vec4 n = detail_on > 0.0 ? textureLod(detail_texture, world * 0.25, ground_lod) : vec4(0.5);
	float rag = (n.r - 0.5) * 3.0 * field_rag;
	float outer = max(width + rag * clamp(width / 0.12, 0.0, 1.0), 0.0);
	// At a ford there is no bank: its bed runs on over the land.
	outer = mix(outer, ford_bank_reach + rag, ford_bank);
	// Where the bank narrows to nothing, the grass overhangs the water in
	// tufts instead of stopping on the line.
	float overhang = field_overhang * clamp(n.g * 1.2 - 0.3, 0.0, 1.0);
	float grass_at = mix(-overhang, outer, smoothstep(0.03, 0.14, outer));
	grass_at = mix(grass_at, outer, ford_bank);
	if (d > grass_at + grass_fade) {
		discard;
	}
	// The water's own edge wanders over the earth: the water laps it.
	float lap = (n.b - 0.5) * 3.0 * field_lap;
	float dw = d - lap * smoothstep(0.02, 0.15, outer) - ford_water_reach * ford_bank;
	// Half a pixel and a half, in tiles, so a line is never thinner than
	// that.
	float px = 0.75 * fwidth(d);
	vec3 colour;
	if (dw < 0.0) {
		vec2 runs = flow_of(tile) * 0.8;
		colour = water_at(TEXTURE, world, runs, now) * base_tint;
		colour *= mix(vec3(1.0), deep_tint, smoothstep(0.3, deep_reach, -dw));
		float near = 1.0 - smoothstep(0.0, field_light_reach, -dw);
		colour = mix(colour, colour * shore_tint, near * field_light);
		if (has_bed && (tile.a > 0.02 || ford_bank > 0.0)) {
			colour = ford_over_bed(colour, world, -dw, tile.a, ford_bank, now);
		}
	} else {
		// The bank's matter, seamless, four metres a texture like the
		// ground: earth, or sand on a beach.
		vec2 m_uv = world * 0.25;
		vec3 earth = textureLod(earth_texture, m_uv, ground_lod).rgb;
		if (has_sand && tile.b > 0.0) {
			earth = mix(earth, textureLod(sand_texture, m_uv + vec2(0.37, 0.11), ground_lod).rgb, tile.b);
		}
		earth *= field_earth_tint;
		earth *= mix(field_wet, 1.0, smoothstep(0.0, 0.14, dw));
		earth *= mix(1.0, field_shade, smoothstep(outer - 0.14, outer, d));
		if (ford_bank > 0.0) {
			// The ford's bed, damp, wetter near the water.
			vec3 bed = textureLod(bed_texture, m_uv, ground_lod).rgb * mix(0.8, 1.0, smoothstep(0.0, 0.5, dw));
			earth = mix(earth, bed, ford_bank);
		}
		colour = earth;
	}
	// The water's edge: darker, melting into the water.
	float line = (1.0 - smoothstep(0.0, max(field_line, px), abs(dw + field_line_into))) * field_line_strength * (1.0 - ford_bank);
	colour = mix(colour, colour * 0.45, line);
	// The grass's own outline, against the earth or the water.
	float ink = 1.0 - smoothstep(0.0, max(0.03, px), abs(d - grass_at + 0.015));
	colour = mix(colour, field_ink, ink * 0.85 * (1.0 - ford_bank));
	// The grass's edge over a ford's bank: ragged by the detail as it
	// fades, so it comes back in tufts.
	float tuft_in = ((n.g - 0.5) * 1.2 + (n.r - 0.5) * 0.8) * grass_fade * ford_bank;
	float a = 1.0 - smoothstep(grass_at + tuft_in, grass_at + tuft_in + max(grass_fade, px), d);
	COLOR = vec4(colour, a);
}
