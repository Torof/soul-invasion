class_name ShorePieces
## The bank as pieces: the land/water boundary is traced as lines, then
## laid with pieces one tile long, end to end, in one of sixteen headings
## (22.5 degrees apart). A straight run keeps its heading until a bend,
## and a bend is a piece of its own (an arc from one heading to the
## next); sharper bends (two or three steps) exist for tight corners.
## A piece never passes through a whole tile: every point of it lies on
## a boundary tile, one that touches the other matter.
##
## The pieces are laid by a beam search along each line, so the whole
## bank is a chain of pieces that fits the tiles. Pure functions, so the
## tests and the bench use them on any little map; the world view lays
## them over everything it knows, so chunks join without a seam.
##
## Headings: index 0 points down the tile grid (+y), 4 along +x; the
## water is always on the right of the direction of travel.

## Unit vectors of the sixteen headings, in tile units.
static var HEADINGS: Array = _headings()

## Points sampled along a piece, for the fit and the drawing.
const SAMPLES := 6
## Candidates kept at each step of the beam search.
const BEAM := 24
## Cost of a bend of one step; sharper bends cost it per step.
const BEND_COST := 0.3
## A chain is done once it has travelled this share of the line's
## length and ends this close to the line's end.
const DONE_SHARE := 0.85
const DONE_REACH_OPEN := 0.6
const DONE_REACH_CLOSED := 0.3


static func _headings() -> Array:
	var out: Array = []
	for i in 16:
		var a := deg_to_rad(i * 22.5)
		out.append(Vector2(sin(a), cos(a)))
	return out


## The land/water boundary between tiles `lo` (inclusive) and `hi`
## (exclusive), as polylines in tile units (a tile's centre is at integer
## coordinates). `is_known` and `is_water` take a Vector2i tile. Unknown
## tiles stop the lines, so they end at the edge of the known world.
static func lines(is_known: Callable, is_water: Callable, lo: Vector2i, hi: Vector2i) -> Array:
	# Marching squares on cells between four tile centres.
	var segments: Array = []
	for gy in range(lo.y, hi.y - 1):
		for gx in range(lo.x, hi.x - 1):
			var c00 := Vector2i(gx, gy)
			var c10 := Vector2i(gx + 1, gy)
			var c01 := Vector2i(gx, gy + 1)
			var c11 := Vector2i(gx + 1, gy + 1)
			if not (is_known.call(c00) and is_known.call(c10) and is_known.call(c01) and is_known.call(c11)):
				continue
			var code := 0
			if is_water.call(c00):
				code += 8
			if is_water.call(c10):
				code += 4
			if is_water.call(c11):
				code += 2
			if is_water.call(c01):
				code += 1
			var top := Vector2(gx + 0.5, gy)
			var right := Vector2(gx + 1, gy + 0.5)
			var bottom := Vector2(gx + 0.5, gy + 1)
			var left := Vector2(gx, gy + 0.5)
			match code:
				1, 14:
					segments.append([left, bottom])
				2, 13:
					segments.append([bottom, right])
				3, 12:
					segments.append([left, right])
				4, 11:
					segments.append([top, right])
				5:
					segments.append([top, right])
					segments.append([left, bottom])
				6, 9:
					segments.append([top, bottom])
				7, 8:
					segments.append([left, top])
				10:
					segments.append([left, top])
					segments.append([bottom, right])
	# Join the segments into polylines by their shared ends.
	var at_end: Dictionary = {}
	for i in segments.size():
		for p in segments[i]:
			var k := _key(p)
			if not at_end.has(k):
				at_end[k] = []
			at_end[k].append(i)
	var used: Dictionary = {}
	var out: Array = []
	for start in segments.size():
		if used.has(start):
			continue
		var line: Array = [segments[start][0], segments[start][1]]
		used[start] = true
		for direction in 2:
			while true:
				var tip: Vector2 = line[line.size() - 1] if direction == 0 else line[0]
				var next := -1
				for i in at_end.get(_key(tip), []):
					if not used.has(i):
						next = i
						break
				if next < 0:
					break
				used[next] = true
				var seg: Array = segments[next]
				var other: Vector2 = seg[1] if _key(seg[0]) == _key(tip) else seg[0]
				if direction == 0:
					line.append(other)
				else:
					line.insert(0, other)
		out.append(line)
	return out


static func _key(p: Vector2) -> Vector2i:
	return Vector2i(int(round(p.x * 2.0)), int(round(p.y * 2.0)))


## Is the line closed on itself (a pond, an island)?
static func is_closed(line: Array) -> bool:
	return line.size() > 2 and line[0].distance_to(line[line.size() - 1]) < 0.01


## The line turned so that the water lies on the right of the direction
## of travel, and, when closed, started at its northernmost point: the
## same line always gives the same pieces, whoever lays it.
static func orient(line: Array, is_water: Callable) -> Array:
	var votes := 0
	for i in range(line.size() - 1):
		var d: Vector2 = (line[i + 1] - line[i]).normalized()
		var right := Vector2(-d.y, d.x)
		var c: Vector2 = (line[i] + line[i + 1]) * 0.5
		var plus := c + right * 0.7
		var minus := c - right * 0.7
		var pw: bool = is_water.call(Vector2i(int(round(plus.x)), int(round(plus.y))))
		var mw: bool = is_water.call(Vector2i(int(round(minus.x)), int(round(minus.y))))
		if pw and not mw:
			votes += 1
		elif mw and not pw:
			votes -= 1
	var out := line.duplicate()
	if votes < 0:
		out.reverse()
	if is_closed(out):
		out.pop_back()
		var first := 0
		for i in out.size():
			var p: Vector2 = out[i]
			var q: Vector2 = out[first]
			if p.y < q.y or (p.y == q.y and p.x < q.x):
				first = i
		out = out.slice(first) + out.slice(0, first)
		out.append(out[0])
	return out


## The points of a piece from `a` in heading `d`, turning by `turn`
## steps (0 for a straight piece): a straight line one tile long, or an
## arc that ends half a tile along the old heading plus half along the
## new one.
static func path_points(a: Vector2, d: int, turn: int) -> PackedVector2Array:
	var out := PackedVector2Array()
	var h0: Vector2 = HEADINGS[d]
	if turn == 0:
		for i in SAMPLES + 1:
			out.append(a + h0 * (float(i) / SAMPLES))
		return out
	var h1: Vector2 = HEADINGS[(d + turn + 16) % 16]
	var end := a + h0 * 0.5 + h1 * 0.5
	var ctrl := a + h0 * 0.5
	for i in SAMPLES + 1:
		var t := float(i) / SAMPLES
		out.append(a * (1.0 - t) * (1.0 - t) + ctrl * 2.0 * (1.0 - t) * t + end * t * t)
	return out


## Lays pieces along an oriented `line` (tile units). `is_boundary` takes
## a Vector2i tile and says whether a piece may pass there. Returns
## [{"from", "to": Vector2, "dir": int (heading at the start), "turn":
## int, "pts": PackedVector2Array}].
static func lay(line: Array, is_boundary: Callable) -> Array:
	if line.size() < 2:
		return []
	var closed := is_closed(line)
	var a: Vector2 = line[0]
	var b: Vector2 = line[1]
	var d0 := heading_of(b - a)
	var goal: Vector2 = line[0] if closed else line[line.size() - 1]
	var total := 0.0
	for i in range(line.size() - 1):
		total += line[i].distance_to(line[i + 1])
	var index := _index(line)
	# A candidate: [score, point, heading, pieces, travelled].
	var beam: Array = [[0.0, a, d0, [], 0.0]]
	var reach := DONE_REACH_CLOSED if closed else DONE_REACH_OPEN
	for _step in int(total * 1.6) + 10:
		var done: Array = []
		for c in beam:
			if c[4] >= total * DONE_SHARE and c[1].distance_to(goal) <= reach:
				done.append(c)
		if not done.is_empty():
			return _best(done)[3]
		var next: Array = []
		for c in beam:
			var p: Vector2 = c[1]
			var d: int = c[2]
			for turn in [0, 1, -1, 2, -2, 3, -3]:
				var pts := path_points(p, d, turn)
				if not _legal(pts, is_boundary):
					continue
				var e := 0.0
				for q in pts:
					e = maxf(e, _distance(q, line, index))
				var pieces: Array = c[3].duplicate()
				pieces.append({"from": p, "to": pts[pts.size() - 1], "dir": d, "turn": turn, "pts": pts})
				next.append([c[0] + e * e + absi(turn) * BEND_COST, pts[pts.size() - 1], (d + turn + 16) % 16, pieces, c[4] + 1.0])
		if next.is_empty():
			break
		next.sort_custom(func(x: Array, y: Array) -> bool: return x[0] < y[0])
		beam = next.slice(0, BEAM)
	return _best(beam)[3]


static func _best(candidates: Array) -> Array:
	var best: Array = candidates[0]
	for c in candidates:
		if c[0] < best[0]:
			best = c
	return best


static func _legal(pts: PackedVector2Array, is_boundary: Callable) -> bool:
	for q in pts:
		if not is_boundary.call(Vector2i(int(round(q.x)), int(round(q.y)))):
			return false
	return true


## The heading closest to a direction.
static func heading_of(v: Vector2) -> int:
	var best := 0
	var best_dot := -INF
	for i in 16:
		var dot: float = HEADINGS[i].dot(v)
		if dot > best_dot:
			best_dot = dot
			best = i
	return best


## Segments of the line by the tile cell they touch, so a point's
## distance to the line only looks at the segments around it.
static func _index(line: Array) -> Dictionary:
	var cells: Dictionary = {}
	for i in range(line.size() - 1):
		var a: Vector2 = line[i]
		var b: Vector2 = line[i + 1]
		var lo := Vector2i(int(floor(minf(a.x, b.x))), int(floor(minf(a.y, b.y))))
		var hi := Vector2i(int(floor(maxf(a.x, b.x))), int(floor(maxf(a.y, b.y))))
		for cy in range(lo.y, hi.y + 1):
			for cx in range(lo.x, hi.x + 1):
				var k := Vector2i(cx, cy)
				if not cells.has(k):
					cells[k] = PackedInt32Array()
				cells[k].append(i)
	return cells


## Distance from `p` to the nearest segment of the line among those in
## the cells around `p`; far when none is near.
static func _distance(p: Vector2, line: Array, index: Dictionary) -> float:
	var best := 9.0
	var c := Vector2i(int(floor(p.x)), int(floor(p.y)))
	for dy in range(-1, 2):
		for dx in range(-1, 2):
			var segs: PackedInt32Array = index.get(c + Vector2i(dx, dy), PackedInt32Array())
			for i in segs:
				var a: Vector2 = line[i]
				var b: Vector2 = line[i + 1]
				var ab := b - a
				var l2 := ab.length_squared()
				var t := 0.0 if l2 == 0.0 else clampf((p - a).dot(ab) / l2, 0.0, 1.0)
				best = minf(best, p.distance_to(a + ab * t))
	return best


## Lays every line of a boundary: the lines traced between `lo` and
## `hi`, oriented, laid. `is_known`, `is_water` take a Vector2i tile.
static func lay_all(is_known: Callable, is_water: Callable, lo: Vector2i, hi: Vector2i) -> Array:
	var is_boundary := func(t: Vector2i) -> bool:
		var w: bool = is_water.call(t)
		for n in [Vector2i(1, 0), Vector2i(-1, 0), Vector2i(0, 1), Vector2i(0, -1)]:
			if is_known.call(t + n) and is_water.call(t + n) != w:
				return true
		return false
	var pieces: Array = []
	for raw in lines(is_known, is_water, lo, hi):
		if raw.size() < 2:
			continue
		pieces.append_array(lay(orient(raw, is_water), is_boundary))
	return pieces


## The name of a piece's image: its heading and its bend, the bend named
## by where it goes (towards the water, which is on the right, or away
## from it). Sharp bends (two or three steps) have no image: "".
static func piece_name(d: int, turn: int) -> String:
	if turn == 0:
		return "straight_%02d" % d
	if absi(turn) != 1:
		return ""
	var h: Vector2 = HEADINGS[d]
	var right := Vector2(-h.y, h.x)
	var next: Vector2 = HEADINGS[(d + turn + 16) % 16]
	return "bend_%02d_%s" % [d, "towater" if next.dot(right) > 0.0 else "toland"]


## The point a piece's image is centred on: its middle sample. The
## images were made around it (dev/piece_schematics.gd), 128 px square
## at scale 1.
static func middle(piece: Dictionary) -> Vector2:
	var pts: PackedVector2Array = piece["pts"]
	return pts[pts.size() / 2]


## The name of a straight piece's slope, for the tally and the colours:
## how far its heading is from the tile grid's vertical.
static func slope_name(d: int) -> String:
	var k := d % 16
	k = mini(k, 16 - k)
	if k > 4:
		k = 8 - k
	return ["vertical", "22deg", "45deg", "67deg", "horizontal"][k]


const SLOPE_COLORS := {
	"vertical": Color(0.12, 0.12, 0.12),
	"22deg": Color(0.08, 0.35, 0.82),
	"45deg": Color(0.82, 0.24, 0.12),
	"67deg": Color(0.51, 0.12, 0.67),
	"horizontal": Color(0.0, 0.55, 0.35),
}
const BEND_COLOR := Color(0.0, 0.6, 0.27)
const SHARP_COLOR := Color(0.78, 0.47, 0.0)


## Draws pieces as lines on `canvas`, in screen space relative to the
## canvas' origin; only those whose middle lies in [lo, hi) tiles.
static func draw(canvas: CanvasItem, pieces: Array, lo: Vector2, hi: Vector2, width: float = 3.0) -> void:
	for piece in pieces:
		var pts: PackedVector2Array = piece["pts"]
		var mid: Vector2 = pts[pts.size() / 2]
		if mid.x < lo.x or mid.y < lo.y or mid.x >= hi.x or mid.y >= hi.y:
			continue
		var screen := PackedVector2Array()
		for q in pts:
			screen.append(ChunkView.tile_point_to_screen(q))
		var turn: int = piece["turn"]
		var color: Color
		if turn == 0:
			color = SLOPE_COLORS[slope_name(piece["dir"])]
		else:
			color = BEND_COLOR if absi(turn) == 1 else SHARP_COLOR
		canvas.draw_polyline(screen, color, width, true)
		var end := screen[screen.size() - 1]
		canvas.draw_circle(end, width * 0.9, Color.WHITE)
		canvas.draw_arc(end, width * 0.9, 0.0, TAU, 12, Color.BLACK, 1.0, true)
