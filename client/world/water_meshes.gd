class_name WaterMeshes
extends RefCounted
## The river as two meshes a chunk (prototype, 11 November 2026): a water
## mesh over the water far from any bank, drawn by a shader of two reads,
## and a bank mesh over the band along the water's line, drawn by a shader
## of three to five reads. Nothing is searched by the pixel: what is still
## is computed once here, on a worker thread, and carried by the vertices.
##
## The water's line is the same as today's (ShoreField): the tiles' water
## mask smoothed by three box blurs and turned into a signed distance in
## tiles, positive on the land. Here the distance is not kept as a texture
## but sampled at a lattice of PX points a tile and carried, in float, by
## the vertices of the bank mesh; the rasteriser interpolates it, which is
## exact across a straight bank and within a pixel at the bends. Two
## chunks compute the same numbers for the lattice points they share, so
## the meshes join without a seam, and no texture is ever uploaded.
##
## Per vertex, the water mesh carries the current (direction and pace,
## averaged so it turns smoothly), the depth of the water (today a proxy
## from the distance to the line, the server's byte when it is sent) and
## the ford; the bank mesh carries the distance to the line, the bank's
## width there after its wander, its beaches and their pinch (today's rule,
## computed once per vertex instead of once per pixel), the sand, the ford
## and the current. The cells are those of the dual grid, between four tile
## centres: a cell is deep (the water mesh) where the whole of it lies more
## than DEEP tiles into the water, a band cell (the bank mesh) where the
## bank can show, nothing elsewhere.

## Lattice points a tile, either way (0.25 tile apart).
const PX := 4
## Tiles read around the chunk: the blur's reach, and the beaches' search
## along the bank past it.
const MARGIN := ChunkView.READ
## A cell deeper than this everywhere is water alone: nothing of the bank
## (its line, its lap, its light) reaches it.
const DEEP := 1.0
## Land this far from the line can carry the bank (the widest beach and
## its tufts); near a ford, farther (the bed runs over the land and the
## grass comes back slowly).
const LAND_REACH := 1.2
const LAND_REACH_FORD := 2.2
## Depth carried by the vertices: tiles from the line, over this many.
const DEPTH_TILES := 3.0
## The bank's profile, today's dials of ground.gdshader.
const WIDTH_MIN := 0.0
const WIDTH_MAX := 0.55
const VARY := 9.0
const BEACH := 14.0
const BEACH_COVER := 0.25
const BEACH_WIDE := 1.6
const BEACH_MIN := 3.0
const BEACH_REACH := 3.0
const BEACH_SAMPLES := 8
const FLOW_MAX := ShoreField.FLOW_MAX

## Builds under way (the loading screen waits for them).
static var busy := 0
## Convolution weights by kind and chunk size (made on the main thread).
static var _weights: Dictionary = {}
## The noise the bank's profile reads (the ground shader's noise image,
## 256 × 256, as floats), made once on the main thread.
static var _noise: PackedFloat32Array
const NOISE_SIDE := 256


## What `build` needs, gathered on the main thread from the chunk's
## window of codes (`win`: ChunkView._window, MARGIN tiles around) and the
## currents (`current`: Vector2i -> Vector2 in metres a second). Empty when
## the block holds no water at all.
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
	var map_size := ShoreField.map_size
	for ty in n:
		for tx in n:
			var at := start + Vector2i(tx, ty)
			var inside := at.x >= 0 and at.y >= 0 and at.x < map_size.x and at.y < map_size.y
			var here := ty * n + tx
			var code: int = win[here] if inside else TerrainGrid.UNKNOWN
			if code == TerrainGrid.UNKNOWN:
				# Past the map or not sent yet: the nearest tile of the chunk
				# goes on, so a river runs out of view instead of ending in a
				# bank (ShoreField's rule).
				var near := Vector2i(clampi(at.x, lo.x, lo.x + size - 1), clampi(at.y, lo.y, lo.y + size - 1))
				near = Vector2i(clampi(near.x, 0, map_size.x - 1), clampi(near.y, 0, map_size.y - 1))
				near = Vector2i(clampi(near.x, start.x, start.x + n - 1), clampi(near.y, start.y, start.y + n - 1))
				code = win[(near.y - start.y) * n + near.x - start.x]
			var water := ShoreField.is_water(code)
			if code == TerrainGrid.BROOK and inside and tx > 0 and ty > 0 and tx < n - 1 and ty < n - 1:
				# A brook's last tile opens the bank in a small inlet.
				for d: int in [1, -1, n, -n, n + 1, n - 1, -n + 1, -n - 1]:
					if ShoreField.is_water(win[here + d]):
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
	return {
		"lo": lo, "size": size, "n": n, "mask": mask, "fords": fords, "flow_x": flow_x, "flow_y": flow_y,
		"any_flow": any_flow, "any_ford": any_ford, "any_land": any_land,
		"weights": [_weights_of("lattice", size), _weights_of("centre", size), _weights_of("ford", size)],
	}


## The convolution weights of one axis (ShoreField._weights_of, with this
## file's margin): "lattice", the field at PX points a tile from a quarter
## tile before the chunk's first tile centre... to a quarter tile past its
## last (4 size + 3 points); "centre" and "ford", the middle of every tile
## from the one before the chunk to the one after (size + 2).
static func _weights_of(kind: String, size: int) -> Array:
	var key := "%s %d" % [kind, size]
	if not _weights.has(key):
		var fine := ShoreField.PX
		var k := ShoreField.kernel(ShoreField.FORD_BLUR, ShoreField.FORD_BLUR_PASSES) if kind == "ford" else ShoreField.kernel(ShoreField.BLUR, ShoreField.BLUR_PASSES)
		var n := size + 2 * MARGIN
		var samples: Array = []
		if kind == "lattice":
			# Point a lies at MARGIN + 0.25 + a / PX tiles into the window: the
			# boundary between the fine texels (MARGIN * fine + a) and the next.
			for a in PX * size + 3:
				var first := MARGIN * fine + a
				samples.append([[first, 0.5], [first + 1, 0.5]])
		else:
			for t in range(-1, size + 1):
				var first := (MARGIN + t) * fine + (fine >> 1) - 1
				samples.append([[first, 0.5], [first + 1, 0.5]])
		var out: Array = []
		var half := (k.size() - 1) >> 1
		for pixels: Array in samples:
			var by_tile := PackedFloat32Array()
			by_tile.resize(n)
			for pa: Array in pixels:
				var p: int = pa[0]
				var a: float = pa[1]
				for q in range(p - half, p + half + 1):
					by_tile[clampi(floori(float(q) / fine), 0, n - 1)] += a * k[q - p + half]
			var lo := 0
			while lo < n - 1 and by_tile[lo] == 0.0:
				lo += 1
			var hi := n - 1
			while hi > lo and by_tile[hi] == 0.0:
				hi -= 1
			out.append([lo, by_tile.slice(lo, hi + 1)])
		_weights[key] = out
	return _weights[key]


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
## colours, indices} in the chunk's screen pixels (empty when the mesh has
## nothing); out[2] the classes of the chunk's size × size cells (0 none,
## 1 water, 2 bank), cell (i, j) spanning tile centres (i, j) to (i + 1,
## j + 1) of the chunk; out[3] the lattice (size * PX + 3 a side) of
## distances, for the tests; out[4] the time it took, in microseconds.
## Touches nothing but its arguments and the static tables, so it may run
## on another thread.
static func build(p: Dictionary, out: Array) -> void:
	var started := Time.get_ticks_usec()
	var size: int = p["size"]
	var n: int = p["n"]
	var weights: Array = p["weights"]
	var l := PX * size + 3
	# The distance to the water's line at the lattice points.
	var d := PackedFloat32Array()
	d.resize(l * l)
	if p["any_land"]:
		var field := ShoreField.convolve(p["mask"], n, weights[0], weights[0])
		var sigma := sqrt(ShoreField.BLUR_PASSES * (pow(2.0 * ShoreField.BLUR + 1.0, 2.0) - 1.0) / 12.0) / ShoreField.PX
		for i in l * l:
			d[i] = -sigma * ShoreField.probit(clampf(field[i], 0.0005, 0.9995))
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
		var near := ShoreField.convolve(p["mask"], n, weights[1], weights[1])
		var fx := ShoreField.convolve(p["flow_x"], n, weights[1], weights[1])
		var fy := ShoreField.convolve(p["flow_y"], n, weights[1], weights[1])
		for i in c * c:
			if near[i] > 0.02:
				flow_x[i] = fx[i] / near[i]
				flow_y[i] = fy[i] / near[i]
	var ford := PackedFloat32Array()
	ford.resize(c * c)
	if p["any_ford"]:
		var f := ShoreField.convolve(p["fords"], n, weights[2], weights[2])
		for i in c * c:
			ford[i] = clampf(f[i], 0.0, 1.0)
	# The cells: deep, band, or nothing.
	var classes := PackedByteArray()
	classes.resize(size * size)
	var any_water_cell := false
	var any_bank_cell := false
	for j in size:
		for i in size:
			var a0 := PX * i + 1
			var b0 := PX * j + 1
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
	out.resize(5)
	out[0] = _mesh(1, classes, size, l, c, lo, d, flow_x, flow_y, ford) if any_water_cell else {}
	out[1] = _mesh(2, classes, size, l, c, lo, d, flow_x, flow_y, ford) if any_bank_cell else {}
	out[2] = classes
	out[3] = d
	out[4] = Time.get_ticks_usec() - started


## The mesh of the cells of class `wanted`: every cell PX × PX quads whose
## vertices are shared along the lattice, so neighbouring cells, and the
## two meshes, meet without a crack.
static func _mesh(wanted: int, classes: PackedByteArray, size: int, l: int, c: int, lo: Vector2i, d: PackedFloat32Array, flow_x: PackedFloat32Array, flow_y: PackedFloat32Array, ford: PackedFloat32Array) -> Dictionary:
	var index := PackedInt32Array()
	index.resize(l * l)
	index.fill(-1)
	var points := PackedVector2Array()
	var uvs := PackedVector2Array()
	var colours := PackedColorArray()
	var indices := PackedInt32Array()
	for j in size:
		for i in size:
			if classes[j * size + i] != wanted:
				continue
			var a0 := PX * i + 1
			var b0 := PX * j + 1
			for bb in PX:
				for aa in PX:
					var v00 := _vertex(index, a0 + aa, b0 + bb, wanted, l, c, lo, d, flow_x, flow_y, ford, points, uvs, colours)
					var v10 := _vertex(index, a0 + aa + 1, b0 + bb, wanted, l, c, lo, d, flow_x, flow_y, ford, points, uvs, colours)
					var v11 := _vertex(index, a0 + aa + 1, b0 + bb + 1, wanted, l, c, lo, d, flow_x, flow_y, ford, points, uvs, colours)
					var v01 := _vertex(index, a0 + aa, b0 + bb + 1, wanted, l, c, lo, d, flow_x, flow_y, ford, points, uvs, colours)
					indices.append_array([v00, v10, v11, v00, v11, v01])
	return {"points": points, "uvs": uvs, "colours": colours, "indices": indices}


## The vertex at lattice point (a, b), made on first use: its place on the
## screen, and what it carries for the mesh's shader.
static func _vertex(index: PackedInt32Array, a: int, b: int, wanted: int, l: int, c: int, lo: Vector2i, d: PackedFloat32Array, flow_x: PackedFloat32Array, flow_y: PackedFloat32Array, ford: PackedFloat32Array, points: PackedVector2Array, uvs: PackedVector2Array, colours: PackedColorArray) -> int:
	var at := b * l + a
	if index[at] >= 0:
		return index[at]
	var id := points.size()
	index[at] = id
	# The point's place: 0.25 + a / PX tiles from the chunk's first tile.
	var w := Vector2(0.25 + float(a) / PX, 0.25 + float(b) / PX)
	var world := Vector2(lo) + w
	points.append(to_screen(world))
	# The current and the ford, between the tile centres around (the
	# centres' array starts a tile before the chunk).
	var u := w + Vector2(0.5, 0.5)
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
	var flow := Vector2(
		flow_x[j0 * c + i0] * w00 + flow_x[j0 * c + i1] * w10 + flow_x[j1 * c + i0] * w01 + flow_x[j1 * c + i1] * w11,
		flow_y[j0 * c + i0] * w00 + flow_y[j0 * c + i1] * w10 + flow_y[j1 * c + i0] * w01 + flow_y[j1 * c + i1] * w11)
	var f := ford[j0 * c + i0] * w00 + ford[j0 * c + i1] * w10 + ford[j1 * c + i0] * w01 + ford[j1 * c + i1] * w11
	var fr := clampf(flow.x / FLOW_MAX * 0.5 + 0.5, 0.0, 1.0)
	var fg := clampf(flow.y / FLOW_MAX * 0.5 + 0.5, 0.0, 1.0)
	var dist := d[at]
	if wanted == 1:
		# The water: the square of the texture under the point, the current,
		# the depth (how far into the water, until the server sends it), the
		# ford.
		uvs.append(world / ChunkView.GROUND_METRES)
		colours.append(Color(fr, fg, clampf(-dist / DEPTH_TILES, 0.0, 1.0), f))
	else:
		# The bank: the distance to the line and the bank's width there, the
		# current, the sand, the ford.
		var profile := _profile(a, b, l, world, dist, d)
		uvs.append(Vector2(dist, profile.x))
		colours.append(Color(fr, fg, profile.y, f))
	return id


## The bank's profile at a lattice point: (the earth's width in tiles,
## the sand 0 to 1), today's rule of ground.gdshader computed once here
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
