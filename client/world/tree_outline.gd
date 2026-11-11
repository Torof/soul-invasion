class_name TreeOutline
extends RefCounted
## The tight outline of a tree image (D39, lightness plan B6): a small
## polygon around everything the sway shader (trees.gdshader) can draw of
## the image, instead of its whole rectangle, of which 13 to 21 % was clear
## and still shaded. Worked out once per image, offline, by
## `tools/tree_outlines.gd` into `art/trees/outlines.json`; the game only
## scales the polygon to each tree's rectangle (Trees.Stand).
##
## What the polygon must hold, in the texture's own texels:
## - every texel with alpha > 0 at every mipmap level the game reads (0 to
##   LEVELS), as the graphics card holds them (the imported, compressed
##   texture: its compression leaks a little alpha into the texels beside,
##   at each level on its own), each grown by the texel the linear filter
##   reaches on either side at that level;
## - clamped to the image (the shader draws nothing outside it), then
##   widened at each row by the largest bend the wind gives it there: the
##   shader bends the top by `(sway + 1.5 gust) * height`, nothing below
##   `still` of the height, growing as the square in between;
## - and a pixel of safety either side and above, for the shortest
##   rectangle a tree is drawn in.
## The polygon is y-monotone (a left chain and a right chain), so the
## test can read its span at every row and check each texel against it.

## The deepest mipmap level the game reads: at the farthest zoom (0.4) in
## the smallest window (1280 x 720, the viewport's own size: the trees'
## texels are then 13.6 to a screen pixel, level 3.8); on a wider window
## the level is lower (3072 wide: 2.5).
const LEVELS := 4
## Texels either side of a level's texel the linear filter reaches, in that
## level's texels (half a texel in truth; a whole one for safety).
const REACH := 1.0
## The shortest rectangle a tree is drawn in, in the canvas's pixels at
## zoom 1 (a hazel at its smallest is 218): the pixel of safety is one of
## its pixels, in texels.
const MIN_RECT_PX := 128.0
## Vertices a polygon may have at most (a triangle array of 14 triangles
## at most, a tree).
const MOST_VERTICES := 16
## Rows the chains are refined until their waste (the area between a chain
## and the texels it holds) is under this share of the image's area.
const WASTE_ENOUGH := 0.004


## For each row of `image` (decompressed with its mipmaps, as the card
## holds it), the leftmost and rightmost texel edge the filter may read
## alpha from, in texels, clamped to the image: {"left":
## PackedFloat32Array, "right": PackedFloat32Array} with left > right where
## the row draws nothing.
static func profile(image: Image) -> Dictionary:
	var img := image.duplicate() as Image
	if img.is_compressed():
		img.decompress()
	if not img.has_mipmaps():
		img.generate_mipmaps()
	if img.get_format() != Image.FORMAT_RGBA8:
		img.convert(Image.FORMAT_RGBA8)
	var w := img.get_width()
	var h := img.get_height()
	var data := img.get_data()
	var left := PackedFloat32Array()
	left.resize(h)
	left.fill(INF)
	var right := PackedFloat32Array()
	right.resize(h)
	right.fill(-INF)
	var levels := mini(LEVELS, img.get_mipmap_count())
	for level in levels + 1:
		var off := img.get_mipmap_offset(level)
		var lw := maxi(w >> level, 1)
		var lh := maxi(h >> level, 1)
		# A level's texel in the image's texels (the chain of an odd size
		# does not halve exactly).
		var sx := float(w) / float(lw)
		var sy := float(h) / float(lh)
		for ry in lh:
			var base := off + ry * lw * 4 + 3
			var lx := -1
			for x in lw:
				if data[base + x * 4] > 0:
					lx = x
					break
			if lx < 0:
				continue
			var rx := lx
			var x2 := lw - 1
			while x2 > lx:
				if data[base + x2 * 4] > 0:
					rx = x2
					break
				x2 -= 1
			var x0 := maxf(0.0, (float(lx) - REACH) * sx)
			var x1 := minf(float(w), (float(rx) + 1.0 + REACH) * sx)
			var y0 := maxi(0, floori((float(ry) - REACH) * sy))
			var y1 := mini(h, ceili((float(ry) + 1.0 + REACH) * sy))
			for y in range(y0, y1):
				left[y] = minf(left[y], x0)
				right[y] = maxf(right[y], x1)
	return {"left": left, "right": right}


## The largest bend the wind gives a row whose top edge is `y` of `h`, in
## texels: what the shader does with Trees.SWAY, GUST and STILL.
static func sway_texels(y: int, h: int) -> float:
	var up := 1.0 - float(y) / float(h)
	var k := clampf((up - Trees.STILL) / (1.0 - Trees.STILL), 0.0, 1.0)
	return (Trees.SWAY + 1.5 * Trees.GUST) * k * k * float(h)


## The span each row must be held within: `profile` widened by the wind's
## bend at that row and the pixel of safety; rows between two drawn rows
## that draw nothing take their neighbours' span, so the polygon stays
## whole. {"left", "right", "top": the first row, "safety": texels}.
static func required(image: Image) -> Dictionary:
	var p := profile(image)
	var left: PackedFloat32Array = p["left"]
	var right: PackedFloat32Array = p["right"]
	var h := left.size()
	var safety := float(h) / MIN_RECT_PX
	var top := -1
	var bottom := -1
	for y in h:
		if left[y] <= right[y]:
			if top < 0:
				top = y
			bottom = y
	if top < 0:
		return {"left": left, "right": right, "top": -1, "safety": safety}
	# Rows that draw nothing between drawn rows: the nearest drawn rows' spans.
	var last := top
	for y in range(top, bottom + 1):
		if left[y] <= right[y]:
			last = y
			continue
		var next := y
		while left[next] > right[next]:
			next += 1
		left[y] = minf(left[last], left[next])
		right[y] = maxf(right[last], right[next])
	# Below the last drawn row, nothing but the foot's own span.
	for y in range(bottom + 1, h):
		left[y] = left[bottom]
		right[y] = right[bottom]
	for y in range(top, h):
		var m := sway_texels(y, h) + safety
		left[y] -= m
		right[y] += m
	# The pixel of safety above: rows with the first drawn row's span.
	var first := top
	top = maxi(0, top - ceili(safety))
	for y in range(top, first):
		left[y] = left[first]
		right[y] = right[first]
	return {"left": left, "right": right, "top": top, "safety": safety}


## The polygon around `required` (from `required`): a left chain down from
## row `top` to row h, the bottom, a right chain back up, the top; at
## most MOST_VERTICES vertices, in texels, rounded outwards to whole
## texels. Empty when the image draws nothing.
static func polygon(req: Dictionary) -> PackedVector2Array:
	var top: int = req["top"]
	if top < 0:
		return PackedVector2Array()
	var left: PackedFloat32Array = req["left"]
	var right: PackedFloat32Array = req["right"]
	var h := left.size()
	# The hull chains refined to several sizes, each brought down to the
	# budget: the smallest polygon of them (a chain refined far is not
	# always the one that reduces best).
	var best: Array = []
	var best_area := INF
	for most: int in [0, 20, 24, 32, 48, 1 << 30]:
		var chains: Array = [
			_chain(left, top, h - 1, -1.0),
			_chain(right, top, h - 1, 1.0),
		]
		_refine(chains, left, right, top, h, most)
		_reduce(chains, left, right, top, h)
		var a := _chain_area(chains[0], -1.0) + _chain_area(chains[1], 1.0)
		if a < best_area:
			best_area = a
			best = chains
	# Down the left chain, along the bottom, up the right chain, along the
	# top; each chain a hundredth of a texel further out, for the rounding.
	var out := PackedVector2Array()
	for v: Vector2 in best[0]:
		out.append(Vector2(v.x - 0.01, v.y))
	var right_chain: PackedVector2Array = best[1]
	for s in range(right_chain.size() - 1, -1, -1):
		out.append(Vector2(right_chain[s].x + 0.01, right_chain[s].y))
	return out


## Refines `chains` (the hull chains of `left` and `right` from row `top`)
## where the waste between a chain and its rows' edges is largest, until
## the waste is small, or the chains hold `most` vertices or more.
static func _refine(chains: Array, left: PackedFloat32Array, right: PackedFloat32Array, top: int, h: int, most: int) -> void:
	var area := float(h) * maxf(1.0, right[top] - left[top])
	for y in range(top, h):
		area = maxf(area, float(h) * (right[y] - left[y]))
	var rounds := 0
	while rounds < 64 and chains[0].size() + chains[1].size() < most:
		rounds += 1
		var best_waste := 0.0
		var best_chain := -1
		var best_segment := -1
		var best_row := -1
		for c in 2:
			var chain: PackedVector2Array = chains[c]
			var edge: PackedFloat32Array = left if c == 0 else right
			var side := -1.0 if c == 0 else 1.0
			for s in chain.size() - 1:
				var a := chain[s]
				var b := chain[s + 1]
				var waste := 0.0
				var deepest := 0.0
				var row := -1
				for y in range(int(a.y), int(b.y)):
					var x := a.x + (b.x - a.x) * (float(y) + 0.5 - a.y) / (b.y - a.y)
					# How far outside the row's edge the chain passes.
					var gap := (x - edge[y]) * side
					waste += gap
					if gap > deepest:
						deepest = gap
						row = y
				if waste > best_waste and row >= 0 and int(b.y) - int(a.y) >= 2:
					best_waste = waste
					best_chain = c
					best_segment = s
					best_row = row
		if best_chain < 0 or best_waste < WASTE_ENOUGH * area:
			break
		var chain: PackedVector2Array = chains[best_chain]
		var edge: PackedFloat32Array = left if best_chain == 0 else right
		var side := -1.0 if best_chain == 0 else 1.0
		var a := chain[best_segment]
		var b := chain[best_segment + 1]
		# Split at the deepest row's bottom edge: a vertex at the span there.
		var split := best_row + 1
		if split >= int(b.y):
			split = int(b.y) - 1
		if split <= int(a.y):
			split = int(a.y) + 1
		var first := _chain(edge, int(a.y), split - 1, side)
		var second := _chain(edge, split, int(b.y) - 1, side)
		# The ends stay the segment's own vertices, or the chains' when
		# those are further out (a vertex moved outwards keeps every row
		# held); the joint likewise the farthest out of the two chains' ends.
		first[0] = a if (a.x - first[0].x) * side > 0.0 else first[0]
		second[-1] = b if (b.x - second[-1].x) * side > 0.0 else second[-1]
		var joint := Vector2(minf(first[-1].x, second[0].x) if side < 0.0 else maxf(first[-1].x, second[0].x), float(split))
		first[-1] = joint
		second[0] = joint
		var refined := PackedVector2Array()
		for s in best_segment:
			refined.append(chain[s])
		refined.append_array(_trim(first, side))
		var tail := _trim(second, side)
		for s in range(1, tail.size()):
			refined.append(tail[s])
		for s in range(best_segment + 2, chain.size()):
			refined.append(chain[s])
		chains[best_chain] = refined


## Takes vertices off `chains` until MOST_VERTICES are left: each time the
## change costing the least area that keeps every row's span held: a
## vertex dropped (its neighbours joined), or an edge taken out (its
## neighbours' lines meeting outside). When neither can be, the longer
## chain becomes its hull again, whose edges can always be taken out.
static func _reduce(chains: Array, left: PackedFloat32Array, right: PackedFloat32Array, top: int, h: int) -> void:
	var rounds := 0
	while chains[0].size() + chains[1].size() > MOST_VERTICES and rounds < 256:
		rounds += 1
		var best_cost := INF
		var best: PackedVector2Array
		var best_chain := -1
		for c in 2:
			var chain: PackedVector2Array = chains[c]
			var edge: PackedFloat32Array = left if c == 0 else right
			var side := -1.0 if c == 0 else 1.0
			var before := _chain_area(chain, side)
			var n := chain.size()
			for i in n:
				# The vertex dropped.
				if i > 0 and i < n - 1:
					var dropped := chain.duplicate()
					dropped.remove_at(i)
					if _valid(dropped[i - 1], dropped[i], edge, side):
						var cost := _chain_area(dropped, side) - before
						if cost < best_cost:
							best_cost = cost
							best = dropped
							best_chain = c
				# The edge from i to i + 1 taken out.
				if i < n - 1:
					var merged := _merge(chain, i, top, h)
					if merged.is_empty():
						continue
					var ok := true
					if i > 0 and not _valid(merged[i - 1], merged[i], edge, side):
						ok = false
					if i + 1 < merged.size() and not _valid(merged[i], merged[i + 1], edge, side):
						ok = false
					if ok:
						var cost := _chain_area(merged, side) - before
						if cost < best_cost:
							best_cost = cost
							best = merged
							best_chain = c
		if best_chain < 0:
			var c := 0 if chains[0].size() >= chains[1].size() else 1
			chains[c] = _chain(left if c == 0 else right, top, h - 1, -1.0 if c == 0 else 1.0)
			continue
		chains[best_chain] = best


## `chain` with the edge from `i` to `i + 1` taken out: the lines of the
## edges either side (the top's and the bottom's horizontals at the ends)
## meet at the vertex that replaces both. Empty when they do not meet
## between the ends' heights.
static func _merge(chain: PackedVector2Array, i: int, top: int, h: int) -> PackedVector2Array:
	var n := chain.size()
	var a0: Vector2
	var a1: Vector2
	var b0: Vector2
	var b1: Vector2
	if i > 0:
		a0 = chain[i - 1]
		a1 = chain[i]
	else:
		a0 = Vector2(0.0, float(top))
		a1 = Vector2(1.0, float(top))
	if i + 2 < n:
		b0 = chain[i + 1]
		b1 = chain[i + 2]
	else:
		b0 = Vector2(0.0, float(h))
		b1 = Vector2(1.0, float(h))
	var met: Variant = Geometry2D.line_intersects_line(a0, a1 - a0, b0, b1 - b0)
	if met == null:
		return PackedVector2Array()
	var p: Vector2 = met
	var low := float(top) if i == 0 else chain[i - 1].y
	var high := float(h) if i + 2 >= n else chain[i + 2].y
	if p.y < low - 1e-6 or p.y > high + 1e-6:
		return PackedVector2Array()
	p.y = clampf(p.y, low, high)
	var out := PackedVector2Array()
	for s in i:
		out.append(chain[s])
	out.append(p)
	for s in range(i + 2, n):
		out.append(chain[s])
	return out


## Whether the segment from `a` down to `b` stays on `side` of every row's
## edge it passes.
static func _valid(a: Vector2, b: Vector2, edge: PackedFloat32Array, side: float) -> bool:
	if b.y < a.y:
		return false
	var h := edge.size()
	var first := clampi(floori(a.y), 0, h - 1)
	var last := clampi(ceili(b.y) - 1, 0, h - 1)
	for y in range(first, last + 1):
		for t: float in [maxf(float(y), a.y), minf(float(y + 1), b.y)]:
			var x := a.x if b.y == a.y else a.x + (b.x - a.x) * (t - a.y) / (b.y - a.y)
			if (x - edge[y]) * side < -1e-4:
				return false
	return true


## The area between `chain` and the vertical through x = 0, on its side:
## what the chain adds to the polygon.
static func _chain_area(chain: PackedVector2Array, side: float) -> float:
	var sum := 0.0
	for s in chain.size() - 1:
		var a := chain[s]
		var b := chain[s + 1]
		sum += (a.x + b.x) / 2.0 * (b.y - a.y)
	return sum * side


## The hull chain of `edge`'s rows `from` to `to` (both in), staying on
## `side` (-1 left, +1 right) of every row's edge: from (edge[from], from)
## down to (edge[to], to + 1), its vertices where the chain turns.
static func _chain(edge: PackedFloat32Array, from: int, to: int, side: float) -> PackedVector2Array:
	var chain := PackedVector2Array()
	for y in range(from, to + 1):
		for q: Vector2 in [Vector2(edge[y], float(y)), Vector2(edge[y], float(y + 1))]:
			while chain.size() >= 2:
				var a := chain[-2]
				var b := chain[-1]
				var cross := (q.x - a.x) * (b.y - a.y) - (q.y - a.y) * (b.x - a.x)
				# b is kept only strictly outside the line from a to q.
				if cross * side < 0.0:
					break
				chain.remove_at(chain.size() - 1)
			chain.append(q)
	return chain


## A chain re-hulled after its joint moved: the vertices that are no
## longer outside the line of their neighbours are dropped.
static func _trim(chain: PackedVector2Array, side: float) -> PackedVector2Array:
	var out := PackedVector2Array()
	for q: Vector2 in chain:
		while out.size() >= 2:
			var a := out[-2]
			var b := out[-1]
			var cross := (q.x - a.x) * (b.y - a.y) - (q.y - a.y) * (b.x - a.x)
			if cross * side < 0.0:
				break
			out.remove_at(out.size() - 1)
		out.append(q)
	return out


## The polygon's span at height `y`: its leftmost and rightmost crossing of
## that horizontal, and how many edges cross it (two for a y-monotone
## polygon, away from its vertices). {"left", "right", "crossings"};
## INF, -INF, 0 where nothing crosses.
static func span(points: PackedVector2Array, y: float) -> Dictionary:
	var lo := INF
	var hi := -INF
	var crossings := 0
	var n := points.size()
	for i in n:
		var a := points[i]
		var b := points[(i + 1) % n]
		if a.y == b.y:
			if a.y == y:
				lo = minf(lo, minf(a.x, b.x))
				hi = maxf(hi, maxf(a.x, b.x))
			continue
		if y < minf(a.y, b.y) or y > maxf(a.y, b.y):
			continue
		var x := a.x + (b.x - a.x) * (y - a.y) / (b.y - a.y)
		lo = minf(lo, x)
		hi = maxf(hi, x)
		# Counted once at a shared vertex: the edge whose top is y.
		if y != maxf(a.y, b.y):
			crossings += 1
	return {"left": lo, "right": hi, "crossings": crossings}


## The area of `points` (shoelace), in texels.
static func area(points: PackedVector2Array) -> float:
	var sum := 0.0
	var n := points.size()
	for i in n:
		var a := points[i]
		var b := points[(i + 1) % n]
		sum += a.x * b.y - b.x * a.y
	return absf(sum) / 2.0


## The polygon of the image's whole rectangle widened for the wind, as
## the shader widened its quad before: for an image without an outline.
static func fallback(w: int, h: int) -> PackedVector2Array:
	var pad := ceilf(sway_texels(0, h) + float(h) / MIN_RECT_PX)
	return PackedVector2Array([Vector2(-pad, 0.0), Vector2(float(w) + pad, 0.0), Vector2(float(w) + pad, float(h)), Vector2(-pad, float(h))])
