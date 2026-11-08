class_name BrookLines
extends RefCounted
## Brooks drawn as lines on the ground (D36): the server sends a brook as
## tiles one or two wide; the client finds their centre line, smooths it,
## and lays a ribbon of shallow water along it, edged with wet earth that
## fades into the ground. The ground under a brook's tiles is the land
## around it. Nothing here decides anything: it only draws what the tiles
## say.
##
## Points are in tile units, the centre of tile (x, y) at (x, y), on the
## ground plane; `to_screen` projects them.

## Water width, as a share of the brook's width in tiles.
const WATER_SHARE := 0.85
## How much the water width varies along the brook, either way.
const WOBBLE := 0.15
## Metres of wet earth on each side of the water, then metres over which
## it fades into the ground.
const EARTH := 0.15
const EARTH_FADE := 0.15
## Opacity of the wet earth.
const EARTH_ALPHA := 0.75
const EARTH_COLOUR := Color(0.30, 0.23, 0.15)
## Metres over which a brook narrows to a point at its spring.
const SPRING_TAPER := 2.0
## Metres a brook runs into the water it joins: over the bank at full
## strength, then fading out, a little wider.
const MOUTH := 0.8
const MOUTH_FADE := 0.8
const MOUTH_FLARE := 1.3
## How much lighter than the river a brook's water is (it is shallow).
const LIGHTEN := 0.25
## Tiles read around a chunk to find the lines crossing it.
const MARGIN := 12

const _N8 := [
	Vector2i(0, -1), Vector2i(1, -1), Vector2i(1, 0), Vector2i(1, 1),
	Vector2i(0, 1), Vector2i(-1, 1), Vector2i(-1, 0), Vector2i(-1, -1),
]


## Screen position of a point in tile units (Iso.tile_to_screen for
## fractions of a tile).
static func to_screen(p: Vector2) -> Vector2:
	return Vector2((p.x - p.y) * Iso.TILE_W / 2.0, (p.x + p.y) * Iso.TILE_H / 2.0)


## The brooks among the tiles of [lo, hi): one entry per stretch between
## two ends or junctions, {"pts": PackedVector2Array, "widths":
## PackedFloat32Array (metres of water at each point), "alphas":
## PackedFloat32Array}. `terrain` gives the terrain of any tile, "" when
## unknown; tiles outside [lo, hi) are read to tell a brook cut by the
## window from a spring.
static func lay(terrain: Callable, lo: Vector2i, hi: Vector2i) -> Array:
	var mask: Dictionary = {}
	for y in range(lo.y, hi.y):
		for x in range(lo.x, hi.x):
			if str(terrain.call(Vector2i(x, y))) == "brook":
				mask[Vector2i(x, y)] = true
	if mask.is_empty():
		return []
	var skeleton := thin(mask)
	var lines: Array = []
	for chain in chains(skeleton):
		var line := _line(chain, skeleton, mask, terrain)
		if not line.is_empty():
			lines.append(line)
	return lines


## Zhang-Suen thinning: the tiles of `mask` worn down to a line one tile
## wide, eight-connected, keeping its ends.
static func thin(mask: Dictionary) -> Dictionary:
	var cells := mask.duplicate()
	var changed := true
	while changed:
		changed = false
		for step in 2:
			var drop: Array = []
			for t: Vector2i in cells:
				var p: Array = []
				for d: Vector2i in _N8:
					p.append(1 if cells.has(t + d) else 0)
				var count := 0
				var flips := 0
				for i in 8:
					count += p[i]
					if p[i] == 0 and p[(i + 1) % 8] == 1:
						flips += 1
				if count < 2 or count > 6 or flips != 1:
					continue
				# p: N, NE, E, SE, S, SW, W, NW.
				if step == 0:
					if p[0] * p[2] * p[4] != 0 or p[2] * p[4] * p[6] != 0:
						continue
				else:
					if p[0] * p[2] * p[6] != 0 or p[0] * p[4] * p[6] != 0:
						continue
				drop.append(t)
			for t in drop:
				cells.erase(t)
			changed = changed or not drop.is_empty()
	return cells


## The neighbours of `t` along a thin line: its four sides, and a
## diagonal only where neither side between reaches it.
static func neighbours(cells: Dictionary, t: Vector2i) -> Array:
	var out: Array = []
	for d: Vector2i in _N8:
		if not cells.has(t + d):
			continue
		if d.x != 0 and d.y != 0 and (cells.has(t + Vector2i(d.x, 0)) or cells.has(t + Vector2i(0, d.y))):
			continue
		out.append(t + d)
	return out


## The stretches of a thin line, each from an end or a junction to the
## next, as tiles in order. A closed loop with no end is left out.
static func chains(cells: Dictionary) -> Array:
	var keys: Array = cells.keys()
	keys.sort()
	var walked: Dictionary = {}
	var out: Array = []
	for t: Vector2i in keys:
		var near := neighbours(cells, t)
		if near.size() == 2:
			continue
		if near.is_empty():
			out.append([t])
			continue
		for n: Vector2i in near:
			if walked.has([t, n]):
				continue
			var chain: Array = [t, n]
			walked[[t, n]] = true
			walked[[n, t]] = true
			var prev := t
			var at := n
			while true:
				var next := neighbours(cells, at)
				if next.size() != 2:
					break
				var go: Vector2i = next[0] if next[0] != prev else next[1]
				walked[[at, go]] = true
				walked[[go, at]] = true
				prev = at
				at = go
				chain.append(at)
				if at == t:
					break
			out.append(chain)
	return out


## How a brook ends at tile `end`: "mouth" into water or a ford, "joint"
## at a junction of brooks, "cut" where the window or the known world
## stops, else "spring".
static func _end_kind(end: Vector2i, skeleton: Dictionary, mask: Dictionary, terrain: Callable) -> String:
	if neighbours(skeleton, end).size() > 1:
		return "joint"
	for d: Vector2i in _N8:
		var t := str(terrain.call(end + d))
		if t == "water" or t == "ford":
			return "mouth"
	for d: Vector2i in _N8:
		var t := str(terrain.call(end + d))
		if t == "" or (t == "brook" and not mask.has(end + d)):
			return "cut"
	return "spring"


## Where the water lies from `end`, as a unit vector.
static func _towards_water(end: Vector2i, terrain: Callable) -> Vector2:
	var sum := Vector2.ZERO
	for d: Vector2i in _N8:
		var t := str(terrain.call(end + d))
		if t == "water" or t == "ford":
			sum += Vector2(d)
	return sum.normalized()


## The brook's width in tiles at `t`: two where the tiles around show a
## band two wide, else one.
static func _tiles_wide(t: Vector2i, terrain: Callable) -> float:
	var count := 0
	for dy in range(-1, 2):
		for dx in range(-1, 2):
			if str(terrain.call(t + Vector2i(dx, dy))) == "brook":
				count += 1
	return 2.0 if count >= 5 else 1.0


## A gentle, steady variation along a brook, -1..1, from the place alone,
## so that two chunks drawing the same point agree.
static func wobble(p: Vector2) -> float:
	return 0.5 * sin(p.x * 1.7 + p.y * 2.3) + 0.5 * sin(p.x * 0.6 - p.y * 1.1)


static func _line(chain: Array, skeleton: Dictionary, mask: Dictionary, terrain: Callable) -> Dictionary:
	if chain.size() < 2:
		return {}
	var first: Vector2i = chain[0]
	var last: Vector2i = chain[chain.size() - 1]
	var kinds := [_end_kind(first, skeleton, mask, terrain), _end_kind(last, skeleton, mask, terrain)]
	var pts := PackedVector2Array()
	var wide := PackedFloat32Array()
	for t: Vector2i in chain:
		pts.append(Vector2(t))
		wide.append(_tiles_wide(t, terrain))
	# The tiles' stairs evened out (a running mean over five points, the
	# ends kept), then two passes of corner cutting.
	for _pass in 2:
		var even := PackedVector2Array([pts[0]])
		for i in range(1, pts.size() - 1):
			var reach := mini(2, mini(i, pts.size() - 1 - i))
			var sum := Vector2.ZERO
			for k in range(-reach, reach + 1):
				sum += pts[i + k]
			even.append(sum / float(2 * reach + 1))
		even.append(pts[pts.size() - 1])
		pts = even
	# Two passes of corner cutting, the ends kept.
	for _pass in 2:
		var p2 := PackedVector2Array([pts[0]])
		var w2 := PackedFloat32Array([wide[0]])
		for i in pts.size() - 1:
			p2.append(pts[i].lerp(pts[i + 1], 0.25))
			p2.append(pts[i].lerp(pts[i + 1], 0.75))
			w2.append(lerpf(wide[i], wide[i + 1], 0.25))
			w2.append(lerpf(wide[i], wide[i + 1], 0.75))
		p2.append(pts[pts.size() - 1])
		w2.append(wide[wide.size() - 1])
		pts = p2
		wide = w2
	var alphas := PackedFloat32Array()
	alphas.resize(pts.size())
	alphas.fill(1.0)
	var earth := alphas.duplicate()
	# Into the water at a mouth: on along its own way, drawn towards the
	# water, over the bank, then fading out; its earth stops at the bank.
	if kinds[0] == "mouth":
		var into := ((pts[0] - pts[mini(3, pts.size() - 1)]).normalized() + _towards_water(first, terrain)).normalized()
		pts.insert(0, pts[0] + into * MOUTH)
		pts.insert(0, pts[0] + into * MOUTH_FADE)
		wide.insert(0, wide[0] * MOUTH_FLARE)
		wide.insert(0, wide[0])
		alphas.insert(0, 1.0)
		alphas.insert(0, 0.0)
		earth.insert(0, 0.0)
		earth.insert(0, 0.0)
	if kinds[1] == "mouth":
		var n := pts.size()
		var into := ((pts[n - 1] - pts[maxi(n - 4, 0)]).normalized() + _towards_water(last, terrain)).normalized()
		var end := pts[n - 1]
		pts.append(end + into * MOUTH)
		pts.append(end + into * (MOUTH + MOUTH_FADE))
		wide.append(wide[wide.size() - 1] * MOUTH_FLARE)
		wide.append(wide[wide.size() - 1])
		alphas.append(1.0)
		alphas.append(0.0)
		earth.append(0.0)
		earth.append(0.0)
	# Arc length from each end, for the springs' taper.
	var along := PackedFloat32Array([0.0])
	for i in range(1, pts.size()):
		along.append(along[i - 1] + pts[i].distance_to(pts[i - 1]))
	var total := along[along.size() - 1]
	var widths := PackedFloat32Array()
	for i in pts.size():
		var w := wide[i] * WATER_SHARE * (1.0 + WOBBLE * wobble(pts[i]))
		if kinds[0] == "spring":
			w *= clampf(along[i] / SPRING_TAPER, 0.0, 1.0)
		if kinds[1] == "spring":
			w *= clampf((total - along[i]) / SPRING_TAPER, 0.0, 1.0)
		widths.append(w)
	return {"pts": pts, "widths": widths, "alphas": alphas, "earth": earth, "ends": kinds}


## The sideways unit vector at point i of `pts`, on the ground plane.
static func _normal(pts: PackedVector2Array, i: int) -> Vector2:
	var a := pts[maxi(i - 1, 0)]
	var b := pts[mini(i + 1, pts.size() - 1)]
	var t := (b - a).normalized()
	return Vector2(-t.y, t.x)


## Whether segment i of a line belongs to the chunk of tiles [lo, hi):
## the tile under its middle does, so each segment is drawn once.
static func owns(line: Dictionary, i: int, lo: Vector2i, hi: Vector2i) -> bool:
	var pts: PackedVector2Array = line["pts"]
	var mid := (pts[i] + pts[i + 1]) / 2.0
	var t := Vector2i(roundi(mid.x), roundi(mid.y))
	return t.x >= lo.x and t.y >= lo.y and t.x < hi.x and t.y < hi.y


## Draws on `canvas` the segments of `lines` that belong to the tiles
## [lo, hi): the wet earth of every line first, then the water, so a
## junction's earth never crosses another brook's water. The water shows
## `texture` with the ground's own mapping (GROUND_METRES per repeat).
static func draw(canvas: CanvasItem, lines: Array, lo: Vector2i, hi: Vector2i, texture: Texture2D, tint: Color) -> void:
	var water := tint.lerp(Color.WHITE, LIGHTEN)
	for layer in 2:
		for line: Dictionary in lines:
			var pts: PackedVector2Array = line["pts"]
			var widths: PackedFloat32Array = line["widths"]
			var alphas: PackedFloat32Array = line["alphas"]
			var earth: PackedFloat32Array = line["earth"]
			for i in pts.size() - 1:
				if not owns(line, i, lo, hi):
					continue
				var n0 := _normal(pts, i)
				var n1 := _normal(pts, i + 1)
				var h0 := widths[i] / 2.0
				var h1 := widths[i + 1] / 2.0
				if layer == 0:
					var e0 := earth[i] * EARTH_ALPHA
					var e1 := earth[i + 1] * EARTH_ALPHA
					if e0 <= 0.0 and e1 <= 0.0:
						continue
					for side in [-1.0, 1.0]:
						_strip(canvas, pts[i], pts[i + 1], n0 * side, n1 * side, h0, h1, h0 + EARTH, h1 + EARTH, e0, e1, e0, e1)
						_strip(canvas, pts[i], pts[i + 1], n0 * side, n1 * side, h0 + EARTH, h1 + EARTH, h0 + EARTH + EARTH_FADE, h1 + EARTH + EARTH_FADE, e0, e1, 0.0, 0.0)
				else:
					var quad := PackedVector2Array([
						pts[i] + n0 * h0, pts[i + 1] + n1 * h1,
						pts[i + 1] - n1 * h1, pts[i] - n0 * h0,
					])
					if _degenerate(quad):
						continue
					var screen := PackedVector2Array()
					var uvs := PackedVector2Array()
					for q in quad:
						screen.append(to_screen(q))
						uvs.append((q + Vector2(0.5, 0.5)) / ChunkView.GROUND_METRES)
					var c0 := Color(water, alphas[i])
					var c1 := Color(water, alphas[i + 1])
					canvas.draw_polygon(screen, PackedColorArray([c0, c1, c1, c0]), uvs, texture)


## One band beside the water, from `in` to `out` metres off the line, with
## the opacity at its inner and outer edges at each end.
static func _strip(canvas: CanvasItem, a: Vector2, b: Vector2, na: Vector2, nb: Vector2, in_a: float, in_b: float, out_a: float, out_b: float, inner_a: float, inner_b: float, outer_a: float, outer_b: float) -> void:
	var quad := PackedVector2Array([a + na * in_a, b + nb * in_b, b + nb * out_b, a + na * out_a])
	if _degenerate(quad):
		return
	var screen := PackedVector2Array()
	for q in quad:
		screen.append(to_screen(q))
	canvas.draw_polygon(screen, PackedColorArray([
		Color(EARTH_COLOUR, inner_a), Color(EARTH_COLOUR, inner_b),
		Color(EARTH_COLOUR, outer_b), Color(EARTH_COLOUR, outer_a),
	]))


## A quad too thin or folded to draw (Godot refuses a degenerate polygon).
static func _degenerate(quad: PackedVector2Array) -> bool:
	var area := 0.0
	for i in 4:
		area += quad[i].cross(quad[(i + 1) % 4])
	return absf(area) < 1e-4 or Geometry2D.triangulate_polygon(quad).is_empty()


## Margin of land drawn either side of a brook's water by the brook
## shader (earth, the grass's edge), in tiles; and the lateral range the
## vertex colour encodes.
const SHADER_MARGIN := 0.4
const LATERAL_RANGE := 2.0


## Draws on `canvas`, whose material is the brook shader
## (world/brook.gdshader), the segments of `lines` that belong to the
## tiles [lo, hi): one band per segment, wider than the water, its vertex
## colour carrying across it the distance from the axis (red) and the
## water's half-width (green), its alpha the fade of a mouth.
static func draw_shaded(canvas: CanvasItem, lines: Array, lo: Vector2i, hi: Vector2i, texture: Texture2D) -> void:
	for line: Dictionary in lines:
		var pts: PackedVector2Array = line["pts"]
		var widths: PackedFloat32Array = line["widths"]
		var alphas: PackedFloat32Array = line["alphas"]
		for i in pts.size() - 1:
			if not owns(line, i, lo, hi):
				continue
			var n0 := _normal(pts, i)
			var n1 := _normal(pts, i + 1)
			var h0 := widths[i] / 2.0
			var h1 := widths[i + 1] / 2.0
			var r0 := h0 + SHADER_MARGIN
			var r1 := h1 + SHADER_MARGIN
			var quad := PackedVector2Array([pts[i] + n0 * r0, pts[i + 1] + n1 * r1, pts[i + 1] - n1 * r1, pts[i] - n0 * r0])
			if _degenerate(quad):
				continue
			var lateral := [r0, r1, -r1, -r0]
			var halves := [h0, h1, h1, h0]
			var fades := [alphas[i], alphas[i + 1], alphas[i + 1], alphas[i]]
			var screen := PackedVector2Array()
			var uvs := PackedVector2Array()
			var colours := PackedColorArray()
			for k in 4:
				screen.append(to_screen(quad[k]))
				uvs.append((quad[k] + Vector2(0.5, 0.5)) / ChunkView.GROUND_METRES)
				colours.append(Color((float(lateral[k]) / LATERAL_RANGE + 1.0) / 2.0, float(halves[k]) / 2.0, 0.0, float(fades[k])))
			canvas.draw_polygon(screen, colours, uvs, texture)

