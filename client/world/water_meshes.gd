class_name WaterMeshes
extends RefCounted
## The river as two meshes a chunk (D36, 11 November 2026): a water mesh
## over the water far from any bank, drawn by a shader of a few reads,
## and a bank mesh over the band along the water's line, drawn by a
## shader of three to five reads. Nothing is searched by the pixel: what
## is still is computed once here, on a worker thread, and carried by the
## vertices.
##
## The water's line: the tiles' water mask smoothed by three box blurs
## (the same smoothing as the distance-field texture drawn before, so the
## line is the one the project lead approved) and turned into a signed
## distance in tiles, positive on the land. The distance is not kept as a
## texture but sampled at a lattice of PX points a tile and carried, in
## float, by the vertices of the bank mesh; the rasteriser interpolates
## it, which is exact across a straight bank and within a pixel at the
## bends. Two chunks compute the same numbers for the lattice points they
## share, so the meshes join without a seam, and no texture is ever
## uploaded. The smoothing is computed as one convolution of the tiles
## themselves with the blurs' kernel, row then column: the same values as
## the blurs, far fewer operations.
##
## Per vertex, the water mesh carries the current (direction and pace,
## averaged so it turns smoothly), the depth of the water (today a proxy
## from the distance to the line, the server's byte when it is sent) and
## the ford; the bank mesh carries the distance to the line, the bank's
## width there after its wander, its beaches and their pinch (computed
## once per vertex instead of once per pixel), the sand, the ford and the
## current. The cells are those of the dual grid, between four tile
## centres: a cell is deep (the water mesh) where the whole of it lies
## more than DEEP tiles into the water, a band cell (the bank mesh) where
## the bank can show, nothing elsewhere.
##
## The meshes are slim: the colours go to the card as bytes (the current,
## the depth, the sand and the ford: a step of 1/255 is below what shows;
## the current's step shifts the drifting pattern by up to a pixel and a
## half at zoom 1, nothing the eye can tell of a moving pattern), the
## distance and the width stay floats (the ink lines are drawn from
## them), the water mesh carries no texture coordinates (its shader takes
## the point of the map from the vertex, as the bank's does), the indices
## are 16-bit and the quarter-tile quads go as triangle strips, a row at
## a time; and the deep water is merged: a half-tile quad of the water
## mesh replaces its four quarter-tile quads where its vertices' values
## are, to a byte, what the four would interpolate. A merged quad beside
## a finer one keeps the finer one's vertex on their shared edge (a fan
## over its boundary), so the mesh never has a T-junction and never a
## crack.

## Lattice points a tile, either way (0.25 tile apart); also the pixels a
## tile the blurs are reckoned in.
const PX := 4
## Box blur radius in pixels, run three times: about 0.9 tile of
## smoothing, enough to round the steps of the tiles away. The current is
## averaged over the same.
const BLUR := 3
const BLUR_PASSES := 3
## Blur for the fords, in pixels: wider boxes, more passes, about 2.4
## tiles of spread.
const FORD_BLUR := 7
const FORD_BLUR_PASSES := 5
## Tiles read around the chunk: the blurs' reach (the kernel spans 2.25
## tiles past the lattice's 0.75), the beaches' search along the bank past
## it, and one more where a brook's last tile looks for the river it
## joins.
const READ := 6
const MARGIN := READ
## Fastest current the vertices carry, in metres a second.
const FLOW_MAX := 2.0
## A cell deeper than this everywhere is water alone: nothing of the bank
## (its line, its lap, its light) reaches it.
const DEEP := 1.0
## Land this far from the line can carry the bank (the widest beach and
## its tufts); near a ford, farther (the bed runs over the land and the
## grass comes back slowly).
const LAND_REACH := 1.2
const LAND_REACH_FORD := 2.2
## Depth carried by the vertices: tiles from the line, over this many:
## the shaders' deep_reach, past which nothing they draw from the depth
## changes, so the vertices beyond carry the same byte and merge.
const DEPTH_TILES := 2.5
## A merged quad's vertices must give, to this much, the values of the
## lattice points it drops: a byte's step, what the colours are sent as.
const MERGE_TOLERANCE := 1.0 / 255.0
## Bytes a vertex costs the card: the place (two floats) and the colour
## (four bytes), and for the bank its distance and width (two floats);
## and an index.
const WATER_VERTEX_BYTES := 12
const BANK_VERTEX_BYTES := 20
const INDEX_BYTES := 2
## The bank's profile, the dials the ground shader had.
const WIDTH_MIN := 0.0
const WIDTH_MAX := 0.55
const VARY := 9.0
const BEACH := 14.0
const BEACH_COVER := 0.25
const BEACH_WIDE := 1.6
const BEACH_MIN := 3.0
const BEACH_REACH := 3.0
const BEACH_SAMPLES := 8

## The map's size in tiles: past it the server sends rock, which here
## counts as unknown so a river runs on out of the map.
static var map_size := Vector2i(256, 256)
## Convolution weights by kind and chunk size (made on the main thread).
static var _weights: Dictionary = {}
## The bank's kernel, for the points asked for one by one (distance_at).
static var _bank_kernel := PackedFloat32Array()
## The noise the bank's profile reads (the ground shader's noise image,
## 256 × 256, as floats), made once on the main thread.
static var _noise: PackedFloat32Array
const NOISE_SIDE := 256


## Is the tile water for the bank: a river or a ford; unknown and
## everything else (a brook, drawn as a ribbon of its own) is land.
static func is_water(code: int) -> bool:
	return code == TerrainGrid.WATER or code == TerrainGrid.FORD


## A current as the protocol codes it, as a vector: its heading h (0 to
## 15) points along (sin(h × 22.5°), cos(h × 22.5°)), its length the speed
## in metres a second.
static func current_vector(code: int) -> Vector2:
	if code <= 0:
		return Vector2.ZERO
	var angle := deg_to_rad(float(code / 256) * 22.5)
	return Vector2(sin(angle), cos(angle)) * (float(code % 256) / 100.0)


## The codes of the tiles `prepare` reads for the chunk at `lo`: READ
## tiles around it, from `terrain` (a Callable of Vector2i -> String). For
## the tests and the benches; the game reads them from TerrainGrid.
static func block_of(terrain: Callable, lo: Vector2i, size: int) -> PackedByteArray:
	var side := size + 2 * READ
	var out := PackedByteArray()
	out.resize(side * side)
	for y in side:
		for x in side:
			out[y * side + x] = TerrainGrid.code_of(str(terrain.call(lo + Vector2i(x - READ, y - READ))))
	return out


## What `build` needs, gathered from the chunk's window of codes (`win`:
## ChunkView._window, MARGIN tiles around) and the currents (`current`:
## Vector2i -> Vector2 in metres a second). Empty when the block holds no
## water at all. Runs on a worker (GroundBuild) once `warm` made the
## tables on the main thread.
static func prepare(win: PackedByteArray, lo: Vector2i, size: int, current: Callable) -> Dictionary:
	if not (win.has(TerrainGrid.WATER) or win.has(TerrainGrid.FORD)):
		return {}
	var n := size + 2 * MARGIN
	var start := lo - Vector2i(MARGIN, MARGIN)
	var mask := PackedFloat32Array()
	mask.resize(n * n)
	var fords := PackedFloat32Array()
	fords.resize(n * n)
	var flow_x := PackedFloat32Array()
	flow_x.resize(n * n)
	var flow_y := PackedFloat32Array()
	flow_y.resize(n * n)
	var any_flow := false
	var any_ford := false
	var any_water := false
	var any_land := false
	for ty in n:
		for tx in n:
			var at := start + Vector2i(tx, ty)
			var inside := at.x >= 0 and at.y >= 0 and at.x < map_size.x and at.y < map_size.y
			var here := ty * n + tx
			var code: int = win[here] if inside else TerrainGrid.UNKNOWN
			if code == TerrainGrid.UNKNOWN:
				# Past the map or not sent yet: the nearest tile of the chunk
				# goes on, so a river runs out of view instead of ending in a
				# bank.
				var near := Vector2i(clampi(at.x, lo.x, lo.x + size - 1), clampi(at.y, lo.y, lo.y + size - 1))
				near = Vector2i(clampi(near.x, 0, map_size.x - 1), clampi(near.y, 0, map_size.y - 1))
				near = Vector2i(clampi(near.x, start.x, start.x + n - 1), clampi(near.y, start.y, start.y + n - 1))
				code = win[(near.y - start.y) * n + near.x - start.x]
			var water := is_water(code)
			if code == TerrainGrid.BROOK and inside and tx > 0 and ty > 0 and tx < n - 1 and ty < n - 1:
				# A brook's last tile opens the bank in a small inlet.
				for d: int in [1, -1, n, -n, n + 1, n - 1, -n + 1, -n - 1]:
					if is_water(win[here + d]):
						water = true
						break
			any_water = any_water or water
			any_land = any_land or not water
			if water:
				mask[here] = 1.0
				if current.is_valid() and inside:
					var runs: Vector2 = current.call(at)
					flow_x[here] = runs.x
					flow_y[here] = runs.y
					any_flow = any_flow or runs != Vector2.ZERO
				if code == TerrainGrid.FORD:
					fords[here] = 1.0
					any_ford = true
	if not any_water:
		return {}
	_make_noise()
	if _bank_kernel.is_empty():
		_bank_kernel = kernel(BLUR, BLUR_PASSES)
	return {
		"lo": lo, "size": size, "n": n, "mask": mask, "fords": fords, "flow_x": flow_x, "flow_y": flow_y,
		"any_flow": any_flow, "any_ford": any_ford, "any_land": any_land,
		"weights": [_weights_of("lattice", size), _weights_of("centre", size), _weights_of("ford", size)],
	}


## Makes, on the main thread, the static tables `prepare` and `build`
## read for chunks of `size` tiles (the noise, the bank's kernel, the
## blurs' weights), so that both may then run on a worker thread.
static func warm(size: int) -> void:
	_make_noise()
	if _bank_kernel.is_empty():
		_bank_kernel = kernel(BLUR, BLUR_PASSES)
	for kind: String in ["lattice", "centre", "ford"]:
		_weights_of(kind, size)


## The kernel of `passes` box blurs of `radius` pixels, as one.
static func kernel(radius: int, passes: int) -> PackedFloat32Array:
	var k := PackedFloat32Array([1.0])
	var width := 2 * radius + 1
	for _p in passes:
		var next := PackedFloat32Array()
		next.resize(k.size() + width - 1)
		for i in k.size():
			for d in width:
				next[i + d] += k[i] / width
		k = next
	return k


## The blurs' spread, in tiles: the standard deviation of the kernel, so a
## smoothed edge reads as a distance (probit).
static func sigma() -> float:
	return sqrt(BLUR_PASSES * (pow(2.0 * BLUR + 1.0, 2.0) - 1.0) / 12.0) / PX


## The convolution weights of one axis: "lattice", the field at PX points
## a tile from a quarter tile before the chunk's first tile centre... to a
## quarter tile past its last (4 size + 6 points); "centre" and "ford",
## the middle of every tile from the one before the chunk to the one
## after (size + 2). Each a list of [first tile, the weights of the tiles
## from it]. Tiles past the window count as the nearest one in it, as
## the blurs' edges did.
static func _weights_of(kind: String, size: int) -> Array:
	var key := "%s %d" % [kind, size]
	if not _weights.has(key):
		var k := kernel(FORD_BLUR, FORD_BLUR_PASSES) if kind == "ford" else kernel(BLUR, BLUR_PASSES)
		var n := size + 2 * MARGIN
		var samples: Array = []
		if kind == "lattice":
			# Point a lies at MARGIN - 0.25 + a / PX tiles into the window: the
			# boundary between the pixels (MARGIN * PX + a - 2) and the next.
			for a in PX * size + 6:
				var first := MARGIN * PX + a - 2
				samples.append([[first, 0.5], [first + 1, 0.5]])
		else:
			for t in range(-1, size + 1):
				var first := (MARGIN + t) * PX + (PX >> 1) - 1
				samples.append([[first, 0.5], [first + 1, 0.5]])
		var out: Array = []
		for pixels: Array in samples:
			out.append(_tile_weights(pixels, k, n))
		_weights[key] = out
	return _weights[key]


## The weights of the tiles of a window `n` tiles wide for a value read
## at `pixels` ([pixel, share] pairs, in pixels of PX a tile) through the
## kernel `k`: [first tile, the weights from it], the zeros trimmed.
static func _tile_weights(pixels: Array, k: PackedFloat32Array, n: int) -> Array:
	var half := (k.size() - 1) >> 1
	var by_tile := PackedFloat32Array()
	by_tile.resize(n)
	for pa: Array in pixels:
		var p: int = pa[0]
		var a: float = pa[1]
		for q in range(p - half, p + half + 1):
			by_tile[clampi(floori(float(q) / PX), 0, n - 1)] += a * k[q - p + half]
	var lo := 0
	while lo < n - 1 and by_tile[lo] == 0.0:
		lo += 1
	var hi := n - 1
	while hi > lo and by_tile[hi] == 0.0:
		hi -= 1
	return [lo, by_tile.slice(lo, hi + 1)]


## `values` (n × n, one a tile) convolved along x with `wx` and along y
## with `wy` (_weights_of): wy.size() rows of wx.size() values.
static func convolve(values: PackedFloat32Array, n: int, wx: Array, wy: Array) -> PackedFloat32Array:
	var columns := wx.size()
	var rows := PackedFloat32Array()
	rows.resize(n * columns)
	for ty in n:
		var row := ty * n
		var empty := true
		for tx in n:
			if values[row + tx] != 0.0:
				empty = false
				break
		if empty:
			continue
		for x in columns:
			var lo: int = wx[x][0]
			var w: PackedFloat32Array = wx[x][1]
			var sum := 0.0
			for k in w.size():
				sum += values[row + lo + k] * w[k]
			rows[ty * columns + x] = sum
	var out := PackedFloat32Array()
	out.resize(wy.size() * columns)
	for y in wy.size():
		var lo: int = wy[y][0]
		var w: PackedFloat32Array = wy[y][1]
		for x in columns:
			var sum := 0.0
			for k in w.size():
				sum += rows[(lo + k) * columns + x] * w[k]
			out[y * columns + x] = sum
	return out


## The inverse of the normal curve (Abramowitz and Stegun 26.2.23, within
## 0.0005), so a smoothed edge reads as a distance.
static func probit(p: float) -> float:
	var q := p if p < 0.5 else 1.0 - p
	var t := sqrt(-2.0 * log(q))
	var x := t - (2.515517 + 0.802853 * t + 0.010328 * t * t) / (1.0 + 1.432788 * t + 0.189269 * t * t + 0.001308 * t * t * t)
	return -x if p < 0.5 else x


## The signed distance to the water's line at any point of the map in
## tiles (a tile x spans [x, x + 1)) within the block `p` (prepare) read
## with its margin, positive on the land: the same smoothing as the
## lattice's, read at that one point (the brooks' mouths ask for a few).
## Infinite where the block holds no river at all.
static func distance_at(p: Dictionary, world: Vector2) -> float:
	if p.is_empty():
		return INF
	if not p["any_land"]:
		return -DEPTH_TILES
	var n: int = p["n"]
	var lo: Vector2i = p["lo"]
	var local := (world - Vector2(lo - Vector2i(MARGIN, MARGIN))) * PX
	var wx := _point_weights(local.x, n)
	var wy := _point_weights(local.y, n)
	var mask: PackedFloat32Array = p["mask"]
	var wxs: PackedFloat32Array = wx[1]
	var wys: PackedFloat32Array = wy[1]
	var sum := 0.0
	for j in wys.size():
		var row := (int(wy[0]) + j) * n + int(wx[0])
		var along := 0.0
		for i in wxs.size():
			along += mask[row + i] * wxs[i]
		sum += along * wys[j]
	return -sigma() * probit(clampf(sum, 0.0005, 0.9995))


## The tiles' weights for the field read at `x` pixels into the window
## (PX a tile): between the two pixels whose centres surround it, as the
## lattice points are read on a pixel's boundary.
static func _point_weights(x: float, n: int) -> Array:
	var s := x - 0.5
	var q0 := floori(s)
	var f := s - q0
	return _tile_weights([[q0, 1.0 - f], [q0 + 1, f]], _bank_kernel, n)


## The noise the ground shader reads, as the texture holds it.
static func _make_noise() -> void:
	if not _noise.is_empty():
		return
	var image := ChunkView.noise_source().get_seamless_image(NOISE_SIDE, NOISE_SIDE)
	_noise.resize(NOISE_SIDE * NOISE_SIDE)
	for y in NOISE_SIDE:
		for x in NOISE_SIDE:
			_noise[y * NOISE_SIDE + x] = image.get_pixel(x, y).r


## The noise at `uv` (texture units, repeating), filtered as the shader
## reads it.
static func noise_at(uv: Vector2) -> float:
	var px := uv.x * NOISE_SIDE - 0.5
	var py := uv.y * NOISE_SIDE - 0.5
	var x0 := floori(px)
	var y0 := floori(py)
	var fx := px - x0
	var fy := py - y0
	var x1 := posmod(x0 + 1, NOISE_SIDE)
	var y1 := posmod(y0 + 1, NOISE_SIDE)
	x0 = posmod(x0, NOISE_SIDE)
	y0 = posmod(y0, NOISE_SIDE)
	var top := _noise[y0 * NOISE_SIDE + x0] * (1.0 - fx) + _noise[y0 * NOISE_SIDE + x1] * fx
	var bottom := _noise[y1 * NOISE_SIDE + x0] * (1.0 - fx) + _noise[y1 * NOISE_SIDE + x1] * fx
	return top * (1.0 - fy) + bottom * fy


## A point of the map in tiles (a tile x spans [x, x + 1)) on the screen.
static func to_screen(w: Vector2) -> Vector2:
	return Vector2((w.x - w.y) * Iso.TILE_W / 2.0, (w.x + w.y) * Iso.TILE_H / 2.0 - Iso.TILE_H / 2.0)


## Builds the two meshes of the block `p` (prepare) into `out`:
## out[0] the water mesh, out[1] the bank mesh, each {points, uvs,
## colours, strip, indices, bytes, bytes_before} in the chunk's screen
## pixels (empty when the mesh has nothing; `strip` the quarter quads as
## one triangle strip, `indices` the merged quads' fans as triangles;
## `uvs` empty for the water mesh; `bytes` what it costs the card, and
## what it would have before packing); out[2] the classes
## of the chunk's size × size cells (0 none, 1 water, 2 bank), cell (i, j)
## spanning tile centres (i, j) to (i + 1, j + 1) of the chunk; out[3]
## the lattice (size * PX + 6 a side, point a lying a / PX - 0.25 tiles
## from the chunk's first tile) of distances, for the tests; out[4] the
## time it took, in microseconds.
## Touches nothing but its arguments and the static tables, so it may run
## on another thread.
static func build(p: Dictionary, out: Array) -> void:
	var started := Time.get_ticks_usec()
	var size: int = p["size"]
	var n: int = p["n"]
	var weights: Array = p["weights"]
	var l := PX * size + 6
	# The distance to the water's line at the lattice points.
	var d := PackedFloat32Array()
	d.resize(l * l)
	if p["any_land"]:
		var field := convolve(p["mask"], n, weights[0], weights[0])
		var spread := sigma()
		for i in l * l:
			d[i] = -spread * probit(clampf(field[i], 0.0005, 0.9995))
	else:
		d.fill(-DEPTH_TILES)
	# The current and the fords at the tile centres, the tile before the
	# chunk to the one after.
	var c := size + 2
	var flow_x := PackedFloat32Array()
	var flow_y := PackedFloat32Array()
	flow_x.resize(c * c)
	flow_y.resize(c * c)
	if p["any_flow"]:
		var near := convolve(p["mask"], n, weights[1], weights[1])
		var fx := convolve(p["flow_x"], n, weights[1], weights[1])
		var fy := convolve(p["flow_y"], n, weights[1], weights[1])
		for i in c * c:
			if near[i] > 0.02:
				flow_x[i] = fx[i] / near[i]
				flow_y[i] = fy[i] / near[i]
	var ford := PackedFloat32Array()
	ford.resize(c * c)
	if p["any_ford"]:
		var f := convolve(p["fords"], n, weights[2], weights[2])
		for i in c * c:
			ford[i] = clampf(f[i], 0.0, 1.0)
	# The cells: deep, band, or nothing.
	var classes := PackedByteArray()
	classes.resize(size * size)
	var any_water_cell := false
	var any_bank_cell := false
	for j in size:
		for i in size:
			var a0 := PX * i + 3
			var b0 := PX * j + 3
			var dmin := INF
			var dmax := -INF
			for bb in PX + 1:
				var row := (b0 + bb) * l + a0
				for aa in PX + 1:
					var v := d[row + aa]
					dmin = minf(dmin, v)
					dmax = maxf(dmax, v)
			var reach := LAND_REACH
			if p["any_ford"]:
				var fmax := maxf(maxf(ford[j * c + i], ford[j * c + i + 1]), maxf(ford[(j + 1) * c + i], ford[(j + 1) * c + i + 1]))
				if fmax > 0.03:
					reach = LAND_REACH_FORD
			if dmax <= -DEEP:
				classes[j * size + i] = 1
				any_water_cell = true
			elif dmin <= reach:
				classes[j * size + i] = 2
				any_bank_cell = true
	var lo: Vector2i = p["lo"]
	var tables := {"l": l, "c": c, "lo": lo, "d": d, "flow_x": flow_x, "flow_y": flow_y, "ford": ford, "size": size}
	out.resize(5)
	out[0] = _mesh(1, classes, tables) if any_water_cell else {}
	out[1] = _mesh(2, classes, tables) if any_bank_cell else {}
	out[2] = classes
	out[3] = d
	out[4] = Time.get_ticks_usec() - started


## The lattice point the half-tile quad (qi, qj) of the chunk (two a
## cell, either way) starts at.
static func _quad_origin(qi: int, qj: int) -> Vector2i:
	return Vector2i(2 * qi + 3, 2 * qj + 3)


## The mesh of the cells of class `wanted`: every cell PX × PX quads whose
## vertices are shared along the lattice, so neighbouring cells, and the
## two meshes, meet without a crack. The water mesh's cells are merged
## into half-tile quads where nothing varies (_merges); a merged quad
## bordering an unmerged one keeps the vertex they share on their edge.
static func _mesh(wanted: int, classes: PackedByteArray, t: Dictionary) -> Dictionary:
	var size: int = t["size"]
	var l: int = t["l"]
	var index := PackedInt32Array()
	index.resize(l * l)
	index.fill(-1)
	var profiles := PackedVector3Array()
	if wanted == 2:
		profiles.resize(l * l)
	var points := PackedVector2Array()
	var uvs := PackedVector2Array()
	var colours := PackedColorArray()
	var indices := PackedInt32Array()
	var v := {"index": index, "points": points, "uvs": uvs, "colours": colours, "profiles": profiles, "wanted": wanted}
	# The quarter quads kept fine, by lattice row: a strip a row.
	var fine_rows: Dictionary = {}
	# Which half-tile quads merge: the water's, away from the chunk's
	# edge (the neighbouring chunk keeps its fine vertices there), where
	# the four quarter quads' values are what the one would interpolate.
	var half := 2 * size
	var merged := PackedByteArray()
	merged.resize(half * half)
	if wanted == 1:
		for qj in range(1, half - 1):
			for qi in range(1, half - 1):
				if classes[(qj >> 1) * size + (qi >> 1)] == 1 and _merges(qi, qj, t):
					merged[qj * half + qi] = 1
	for j in size:
		for i in size:
			if classes[j * size + i] != wanted:
				continue
			for qj in range(2 * j, 2 * j + 2):
				for qi in range(2 * i, 2 * i + 2):
					var o := _quad_origin(qi, qj)
					if merged[qj * half + qi] == 0:
						for bb in 2:
							var b := o.y + bb
							if not fine_rows.has(b):
								fine_rows[b] = []
							(fine_rows[b] as Array).append_array([o.x, o.x + 1])
						continue
					# The merged quad: its corners, and the middle of each edge
					# shared with an unmerged quad (a bank's or a fine water's),
					# as one fan from the first such middle (from a corner, the
					# triangle to the middle of its own edge would be flat and
					# the edge a T-junction), else from a corner.
					var ring := PackedInt32Array()
					var apex := 0
					ring.append(_vertex(v, o.x, o.y, t))
					if _keeps_middle(qi, qj - 1, classes, merged, size):
						apex = ring.size() if apex == 0 else apex
						ring.append(_vertex(v, o.x + 1, o.y, t))
					ring.append(_vertex(v, o.x + 2, o.y, t))
					if _keeps_middle(qi + 1, qj, classes, merged, size):
						apex = ring.size() if apex == 0 else apex
						ring.append(_vertex(v, o.x + 2, o.y + 1, t))
					ring.append(_vertex(v, o.x + 2, o.y + 2, t))
					if _keeps_middle(qi, qj + 1, classes, merged, size):
						apex = ring.size() if apex == 0 else apex
						ring.append(_vertex(v, o.x + 1, o.y + 2, t))
					ring.append(_vertex(v, o.x, o.y + 2, t))
					if _keeps_middle(qi - 1, qj, classes, merged, size):
						apex = ring.size() if apex == 0 else apex
						ring.append(_vertex(v, o.x, o.y + 1, t))
					var m := ring.size()
					for k in range(1, m - 1):
						indices.append_array([ring[apex], ring[(apex + k) % m], ring[(apex + k + 1) % m]])
	# The quarter quads as one strip: along each lattice row, runs of
	# quads side by side, the runs and the rows joined by repeated indices
	# (flat triangles, drawn as nothing).
	var strip := PackedInt32Array()
	var rows: Array = fine_rows.keys()
	rows.sort()
	for b: int in rows:
		var row: Array = fine_rows[b]
		row.sort()
		var last := -2
		for a: int in row:
			if a != last + 1:
				var v0 := _vertex(v, a, b, t)
				if not strip.is_empty():
					strip.append_array([strip[strip.size() - 1], v0])
				strip.append_array([v0, _vertex(v, a, b + 1, t)])
			strip.append_array([_vertex(v, a + 1, b, t), _vertex(v, a + 1, b + 1, t)])
			last = a
	# What the same cells weighed before (D39's record): every quarter
	# quad as triangles, the colours as floats, texture coordinates for
	# both meshes, the indices 32-bit.
	var fine := 0
	var marked := PackedByteArray()
	marked.resize(l * l)
	var cells := 0
	for j in size:
		for i in size:
			if classes[j * size + i] != wanted:
				continue
			cells += 1
			for bb in PX + 1:
				var row := (PX * j + 3 + bb) * l + PX * i + 3
				for aa in PX + 1:
					if marked[row + aa] == 0:
						marked[row + aa] = 1
						fine += 1
	var index_bytes := INDEX_BYTES if points.size() < 65536 else 4
	return {"points": points, "uvs": uvs, "colours": colours, "strip": strip, "indices": indices,
		"bytes": points.size() * (WATER_VERTEX_BYTES if wanted == 1 else BANK_VERTEX_BYTES) + (strip.size() + indices.size()) * index_bytes,
		"bytes_before": fine * 32 + cells * PX * PX * 6 * 4}


## Whether the half-tile quad (qi, qj) beside a merged one is drawn fine
## (a bank cell's, or a water cell's left unmerged), so the vertex in the
## middle of their shared edge exists and the merged quad must keep it.
static func _keeps_middle(qi: int, qj: int, classes: PackedByteArray, merged: PackedByteArray, size: int) -> bool:
	var half := 2 * size
	if qi < 0 or qj < 0 or qi >= half or qj >= half:
		return false
	return classes[(qj >> 1) * size + (qi >> 1)] != 0 and merged[qj * half + qi] == 0


## Whether the four quarter quads of the half-tile quad (qi, qj) of the
## water mesh may be one: the colour at the middle of each of its edges
## is, to a byte, the mean of the edge's ends, and the colour at its
## centre the mean of the two corners its diagonal joins (what the two
## triangles of the one quad interpolate there).
static func _merges(qi: int, qj: int, t: Dictionary) -> bool:
	var o := _quad_origin(qi, qj)
	var c00 := _water_colour(o.x, o.y, t)
	var c20 := _water_colour(o.x + 2, o.y, t)
	var c22 := _water_colour(o.x + 2, o.y + 2, t)
	var c02 := _water_colour(o.x, o.y + 2, t)
	return _near(_water_colour(o.x + 1, o.y, t), c00, c20) \
		and _near(_water_colour(o.x + 2, o.y + 1, t), c20, c22) \
		and _near(_water_colour(o.x + 1, o.y + 2, t), c22, c02) \
		and _near(_water_colour(o.x, o.y + 1, t), c02, c00) \
		and _near(_water_colour(o.x + 1, o.y + 1, t), c00, c22)


static func _near(c: Color, a: Color, b: Color) -> bool:
	var m := (a + b) / 2.0
	return absf(c.r - m.r) <= MERGE_TOLERANCE and absf(c.g - m.g) <= MERGE_TOLERANCE \
		and absf(c.b - m.b) <= MERGE_TOLERANCE and absf(c.a - m.a) <= MERGE_TOLERANCE


## The vertex at lattice point (a, b), made on first use: its place on the
## screen, and what it carries for the mesh's shader.
static func _vertex(v: Dictionary, a: int, b: int, t: Dictionary) -> int:
	var index: PackedInt32Array = v["index"]
	var l: int = t["l"]
	var at := b * l + a
	if index[at] >= 0:
		return index[at]
	var points: PackedVector2Array = v["points"]
	var id := points.size()
	index[at] = id
	var lo: Vector2i = t["lo"]
	# The point's place: a / PX - 0.25 tiles from the chunk's first tile.
	var world := Vector2(lo) + Vector2(float(a) / PX - 0.25, float(b) / PX - 0.25)
	points.append(to_screen(world))
	var uvs: PackedVector2Array = v["uvs"]
	var colours: PackedColorArray = v["colours"]
	if v["wanted"] == 1:
		# The water: the current, the depth (how far into the water, until
		# the server sends it), the ford; its shader takes the point of the
		# map from the vertex.
		colours.append(_water_colour(a, b, t))
	else:
		# The bank: the distance to the line and the bank's width there; the
		# current, the sand, the ford.
		var d: PackedFloat32Array = t["d"]
		var profile := _profile_near(a, b, l, lo, d, v["profiles"])
		var flow := _flow_and_ford(a, b, t)
		uvs.append(Vector2(d[at], profile.x))
		colours.append(Color(flow.x, flow.y, profile.y, flow.z))
	return id


## The water mesh's colour at lattice point (a, b): the current (R, G
## over ±FLOW_MAX), the depth (B, tiles into the water over DEPTH_TILES),
## the ford (A).
static func _water_colour(a: int, b: int, t: Dictionary) -> Color:
	var flow := _flow_and_ford(a, b, t)
	var d: PackedFloat32Array = t["d"]
	var l: int = t["l"]
	return Color(flow.x, flow.y, clampf(-d[b * l + a] / DEPTH_TILES, 0.0, 1.0), flow.z)


## The current (x, y, as the colour carries it) and the ford (z) at
## lattice point (a, b), between the tile centres around (the centres'
## array starts a tile before the chunk).
static func _flow_and_ford(a: int, b: int, t: Dictionary) -> Vector3:
	var c: int = t["c"]
	var flow_x: PackedFloat32Array = t["flow_x"]
	var flow_y: PackedFloat32Array = t["flow_y"]
	var ford: PackedFloat32Array = t["ford"]
	var u := Vector2(float(a) / PX + 0.25, float(b) / PX + 0.25)
	var i0 := clampi(floori(u.x), 0, c - 1)
	var j0 := clampi(floori(u.y), 0, c - 1)
	var i1 := mini(i0 + 1, c - 1)
	var j1 := mini(j0 + 1, c - 1)
	var fx := u.x - i0
	var fy := u.y - j0
	var w00 := (1.0 - fx) * (1.0 - fy)
	var w10 := fx * (1.0 - fy)
	var w01 := (1.0 - fx) * fy
	var w11 := fx * fy
	var x := flow_x[j0 * c + i0] * w00 + flow_x[j0 * c + i1] * w10 + flow_x[j1 * c + i0] * w01 + flow_x[j1 * c + i1] * w11
	var y := flow_y[j0 * c + i0] * w00 + flow_y[j0 * c + i1] * w10 + flow_y[j1 * c + i0] * w01 + flow_y[j1 * c + i1] * w11
	var f := ford[j0 * c + i0] * w00 + ford[j0 * c + i1] * w10 + ford[j1 * c + i0] * w01 + ford[j1 * c + i1] * w11
	return Vector3(clampf(x / FLOW_MAX * 0.5 + 0.5, 0.0, 1.0), clampf(y / FLOW_MAX * 0.5 + 0.5, 0.0, 1.0), f)


## The bank's profile at a lattice point: computed at the points half a
## tile apart (the even ones), once each, and averaged between them at the
## others; the noise it reads varies over tiles, the beaches' ends over
## half a tile. The same points on both sides of a chunk border give the
## same values. `profiles`: x the width, y the sand, z 1 once computed.
static func _profile_near(a: int, b: int, l: int, lo: Vector2i, d: PackedFloat32Array, profiles: PackedVector3Array) -> Vector2:
	var sum := Vector2.ZERO
	var count := 0
	for bb in range(b - (b & 1), b + (b & 1) + 1, 2):
		for aa in range(a - (a & 1), a + (a & 1) + 1, 2):
			var at := bb * l + aa
			if profiles[at].z == 0.0:
				var world := Vector2(lo) + Vector2(float(aa) / PX - 0.25, float(bb) / PX - 0.25)
				var p := _profile(aa, bb, l, world, d[at], d)
				profiles[at] = Vector3(p.x, p.y, 1.0)
			sum += Vector2(profiles[at].x, profiles[at].y)
			count += 1
	return sum / count


## The bank's profile at a lattice point: (the earth's width in tiles,
## the sand 0 to 1), the ground shader's former rule computed once here
## instead of once a pixel: read on the line at the point nearest, so it
## is the same right across the bank; the width wanders along the bank
## and widens on a beach, a run of high noise at least BEACH_MIN tiles
## long, pinching to nothing at the beach's ends.
static func _profile(a: int, b: int, l: int, world: Vector2, dist: float, d: PackedFloat32Array) -> Vector2:
	var grad := Vector2(d[b * l + a + 1] - d[b * l + a - 1], d[(b + 1) * l + a] - d[(b - 1) * l + a])
	var across := grad.normalized() if grad.length() > 1e-5 else Vector2(0.0, 1.0)
	var along := Vector2(-across.y, across.x)
	var on_line := world - across * dist
	var wander := noise_at(on_line * (0.2 / VARY) + Vector2(0.31, 0.77))
	var wander2 := noise_at(on_line * (0.2 / (VARY * 0.4)) + Vector2(0.62, 0.15))
	var width := lerpf(WIDTH_MIN, WIDTH_MAX, smoothstep(0.3, 0.75, wander * 0.75 + wander2 * 0.4))
	var t_beach := 1.0 - BEACH_COVER
	var spacing := BEACH_REACH / BEACH_SAMPLES
	var far := BEACH_SAMPLES * spacing
	var sand := 0.0
	var pinch := 0.0
	var centre := _beach_noise(on_line, along, BEACH_SAMPLES, spacing)
	if centre > t_beach:
		# The run of high noise through this point, followed outwards.
		var k := BEACH_SAMPLES
		var at_k := centre
		var below_k := 0.0
		while k > 0:
			var v := _beach_noise(on_line, along, k - 1, spacing)
			if v <= t_beach:
				below_k = v
				break
			at_k = v
			k -= 1
		var j := BEACH_SAMPLES
		var at_j := centre
		var below_j := 0.0
		while j < 2 * BEACH_SAMPLES:
			var v := _beach_noise(on_line, along, j + 1, spacing)
			if v <= t_beach:
				below_j = v
				break
			at_j = v
			j += 1
		var start := -far
		if k > 0:
			start = (float(k - 1) + (t_beach - below_k) / maxf(at_k - below_k, 1e-4) - BEACH_SAMPLES) * spacing
		var end := far
		if j < 2 * BEACH_SAMPLES:
			end = (float(j) + (t_beach - at_j) / minf(below_j - at_j, -1e-4) - BEACH_SAMPLES) * spacing
		if end - start >= BEACH_MIN:
			var to_end := minf(-start, end)
			sand = smoothstep(0.25, 0.8, to_end)
			pinch = 1.0 - smoothstep(0.0, 0.4, to_end)
	width = lerpf(width, maxf(width, WIDTH_MAX) * BEACH_WIDE, sand)
	width *= 1.0 - pinch
	return Vector2(width, sand)


static func _beach_noise(on_line: Vector2, along: Vector2, k: int, spacing: float) -> float:
	var at := on_line + along * float(k - BEACH_SAMPLES) * spacing
	return noise_at(at * (0.2 / BEACH) + Vector2(0.13, 0.52))
