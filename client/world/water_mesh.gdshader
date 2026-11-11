// The water far from any bank (WaterMeshes): one flowing layer of the
// painted water, read twice (the two phases of its drift along the
// current, cross-faded so the reset never shows), darker and cooler with
// the depth, lighter towards the bank; at a ford, the bed seen through
// the thin water, with its ripples. Everything else comes from the
// vertices: the current, the depth, the ford; the point of the map
// from the vertex's place, so the vertices carry no texture coordinates.
// Written without a blend: the water is opaque, so the target is never
// read back.
shader_type canvas_item;
render_mode blend_disabled;

#include "water_common.gdshaderinc"

varying vec4 tile;
varying vec2 world;

void vertex() {
	tile = COLOR;
	// The point of the map in tiles, from the vertex's place on the screen
	// (the inverse of WaterMeshes.to_screen).
	vec2 px = (MODEL_MATRIX * vec4(VERTEX, 0.0, 1.0)).xy;
	world = vec2(px.x / 64.0 + (px.y + 16.0) / 32.0, (px.y + 16.0) / 32.0 - px.x / 64.0);
}

void fragment() {
	float now = flow_time < 0.0 ? TIME : flow_time;
	vec2 runs = flow_of(tile) * 0.8;
	float depth = tile.b * depth_tiles;
	vec3 colour = water_at(TEXTURE, world, runs, now) * base_tint;
	colour *= mix(vec3(1.0), deep_tint, smoothstep(0.3, deep_reach, depth));
	float near = 1.0 - smoothstep(0.0, field_light_reach, depth);
	colour = mix(colour, colour * shore_tint, near * field_light);
	if (has_bed && tile.a > 0.02) {
		colour = ford_over_bed(colour, world, depth, tile.a, 0.0, now);
	}
	COLOR = vec4(colour, 1.0);
}
