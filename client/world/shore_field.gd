class_name ShoreField
extends RefCounted
## The distance to the water's edge around the view, for the ground
## shader to draw the bank (D36): the tiles' water mask is smoothed, so
## the edge comes out round instead of in steps, and turned into a signed
## distance in tiles, positive on the land. Two shared textures of TILES ×
## TILES tiles laid over the map again and again: the tile (x, y) is at
## (x mod TILES, y mod TILES), so they do not grow with the map, and
## neighbours on the map are neighbours in them. TILES is wider than the
## chunks a client holds, so two of them never share a place. Each chunk
## writes its own block when it is drawn, read with a margin of its
## neighbours so the blocks join.
##
## The bank texture (one byte, PX pixels a tile) carries the distance. The
## water texture (one pixel a tile, the shader's filtering spreading it)
## carries what changes slowly across the water: the fords in R, smoothed
## wider, 1 inside a ford and 0 away from one; where the water runs in G
## and B (from the server's currents), averaged over the water nearby, x
## then y, in metres a second over ±FLOW_MAX, 0.5 for still. Both were
## one RGBA texture at PX pixels a tile, four times the bytes for the
## distance alone and sixteen times for the rest (D39).
##
## The smoothing is three box blurs of the tiles' mask drawn at PX pixels
## a tile. It is computed as one convolution of the tiles themselves with
## the same kernel, row then column, which gives the same values with far
## fewer operations.

## Pixels a tile of the bank texture.
const PX := 4
## Tiles the textures cover either way, then repeat: twenty chunks,
## more than the nineteen a client holds across (the server's view,
## VIEW_RADIUS): two chunks held never share a place in them.
const TILES := 640
## Distances kept, either way, in tiles.
const RANGE := 4.0
## Box blur radius in pixels (of the bank texture), run three times:
## about 0.9 tile of smoothing, enough to round the steps of the tiles
## away. The current is averaged over the same.
const BLUR := 3
const BLUR_PASSES := 3
## Fastest current the field carries, in metres a second.
const FLOW_MAX := 2.0
## Tiles smoothed around a chunk.
const MARGIN := 5
## Tiles read around a chunk: the margin and one more, where a brook's
## last tile looks for the river it joins.
const READ := MARGIN + 1
## Blur for the fords, in pixels of the bank texture: wider boxes, more
## passes, about 2.4 tiles of spread.
const FORD_BLUR := 7
const FORD_BLUR_PASSES := 5

## The map's size in tiles: past it the server sends rock, which here
## counts as unknown so a river runs on out of the map.
static var map_size := Vector2i(256, 256)

## The latest update asked for each block: an update running on another
## core gives up when a newer one for its block has started.
static var _jobs: Dictionary = {}
## Updates under way (the loading screen waits for them).
static var busy := 0
static var _bank_image: Image
static var _bank_texture: ImageTexture
static var _water_image: Image
static var _water_texture: ImageTexture
## Whether the images changed since they were last sent to the textures:
## sent once a frame at most, however many blocks were written.
static var _upload_due := false
## Convolution weights, by kind and chunk size (made on the main thread,
## read by the workers).
static var _weights: Dictionary = {}
## The pieces a block without water lays, by chunk size (_dry_pieces).
static var _dry: Dictionary = {}


static func _make_textures() -> void:
	if _bank_texture != null:
		return
	_bank_image = Image.create(TILES * PX, TILES * PX, false, Image.FORMAT_R8)
	_bank_image.fill(Color(1, 0, 0))  # all land
	_bank_texture = ImageTexture.create_from_image(_bank_image)
	_water_image = Image.create(TILES, TILES, false, Image.FORMAT_RGBA8)
	_water_image.fill(Color(0, 0.5, 0.5, 1))  # no ford, still
	_water_texture = ImageTexture.create_from_image(_water_image)


## The bank's distance, PX pixels a tile.
static func texture() -> ImageTexture:
	_make_textures()
	return _bank_texture


## The fords and the current, a pixel a tile.
static func water_texture() -> ImageTexture:
	_make_textures()
	return _water_texture


## Is the tile water for the bank: a river or a ford; unknown and
## everything else is land.
static func is_water(code: int) -> bool:
	return code == TerrainGrid.WATER or code == TerrainGrid.FORD


## The codes of the tiles `update` reads for the block at `lo`: READ tiles
## around it, from `terrain` (a Callable of Vector2i -> String). For the
## tests and the benches; the game reads them from TerrainGrid.
static func block_of(terrain: Callable, lo: Vector2i, size: int) -> PackedByteArray:
	var side := size + 2 * READ
	var out := PackedByteArray()
	out.resize(side * side)
	for y in side:
		for x in side:
			out[y * side + x] = TerrainGrid.code_of(str(terrain.call(lo + Vector2i(x - READ, y - READ))))
	return out


## Writes the block of tiles [lo, lo + size) from `block`, the codes of
## the tiles from lo - READ, (size + 2 READ) a side (block_of).
## `current`, when valid, gives where the water runs at a tile (a vector
## in metres a second, Vector2i -> Vector2).
## `threaded`: the smoothing runs on another core and the textures are
## written a few frames later; otherwise all at once (the tests).
static func update(block: PackedByteArray, lo: Vector2i, size: int, current := Callable(), threaded := false) -> void:
	busy += 1
	await _update(block, lo, size, current, threaded)
	busy -= 1


static func _update(block: PackedByteArray, lo: Vector2i, size: int, current: Callable, threaded: bool) -> void:
	_make_textures()
	var job := int(_jobs.get(lo, 0)) + 1
	_jobs[lo] = job
	var n := size + 2 * MARGIN
	var side := size + 2 * READ
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
	# Most blocks have no water at all (a brook is water only beside it):
	# nothing to look at tile by tile.
	var dry := not (block.has(TerrainGrid.WATER) or block.has(TerrainGrid.FORD))
	for ty in 0 if dry else n:
		for tx in n:
			var at := start + Vector2i(tx, ty)
			var inside := at.x >= 0 and at.y >= 0 and at.x < map_size.x and at.y < map_size.y
			var here := (ty + 1) * side + tx + 1
			var code: int = block[here] if inside else TerrainGrid.UNKNOWN
			if code == TerrainGrid.UNKNOWN:
				# Unknown (past the map's edge, or not sent yet): the nearest
				# tile of the chunk goes on, so a river runs on out of view
				# instead of being closed by a bank.
				var near := Vector2i(clampi(at.x, lo.x, lo.x + size - 1), clampi(at.y, lo.y, lo.y + size - 1))
				near = Vector2i(clampi(near.x, 0, map_size.x - 1), clampi(near.y, 0, map_size.y - 1))
				# (Still within the block, should the chunk lie past the map.)
				near = Vector2i(clampi(near.x, lo.x - READ, lo.x + size + READ - 1), clampi(near.y, lo.y - READ, lo.y + size + READ - 1))
				code = block[(near.y - lo.y + READ) * side + near.x - lo.x + READ]
			var water := is_water(code)
			if code == TerrainGrid.BROOK and inside:
				# A brook's last tile at a river opens the bank in a small
				# inlet, so the brook flows in rather than meeting a wall.
				for d: int in [1, -1, side, -side, side + 1, side - 1, -side + 1, -side - 1]:
					if is_water(block[here + d]):
						water = true
						break
			any_water = any_water or water
			any_land = any_land or not water
			if water:
				var i := ty * n + tx
				mask[i] = 1.0
				if current.is_valid() and inside:
					var runs: Vector2 = current.call(at)
					flow_x[i] = runs.x
					flow_y[i] = runs.y
					any_flow = any_flow or runs != Vector2.ZERO
				if code == TerrainGrid.FORD:
					fords[i] = 1.0
					any_ford = true
	var inner := size * PX
	var bank: Image
	var waters: Image
	if not any_water:
		# No water in the block nor its margin (most of them): all land,
		# no ford, still water, without smoothing or encoding a pixel: the
		# pieces made once for the size are laid as they are.
		var pieces := _dry_pieces(size)
		bank = pieces[0]
		waters = pieces[1]
	else:
		var weights := [_weights_of("bank", size), _weights_of("centre", size), _weights_of("ford", size)]
		var out := [PackedByteArray(), PackedByteArray()]
		if threaded:
			# In the game the smoothing and the encoding run on another core
			# (they read and write only these arrays); the result is laid
			# when ready, unless a newer update for this block came meanwhile.
			var task := WorkerThreadPool.add_task(_smooth.bind(out, mask, fords, flow_x, flow_y, n, size, weights, any_flow, any_ford, any_land))
			var tree := Engine.get_main_loop() as SceneTree
			while tree != null and not WorkerThreadPool.is_task_completed(task):
				await tree.process_frame
			WorkerThreadPool.wait_for_task_completion(task)
			if _jobs.get(lo, 0) != job:
				return
		else:
			_smooth(out, mask, fords, flow_x, flow_y, n, size, weights, any_flow, any_ford, any_land)
		bank = Image.create_from_data(inner, inner, false, Image.FORMAT_R8, out[0])
		waters = Image.create_from_data(size, size, false, Image.FORMAT_RGBA8, out[1])
	_blit(_bank_image, bank, lo * PX, inner, TILES * PX)
	_blit(_water_image, waters, lo, size, TILES)
	if not _upload_due:
		_upload_due = true
		_upload.call_deferred()


## The pieces a block without water lays, for a chunk `size` tiles a
## side: [the bank, all land; the water, no ford and still]. Made once a
## size and laid as they are: a thousand small arrays appended a chunk
## were most of what a dry block cost (D39).
static func _dry_pieces(size: int) -> Array:
	if not _dry.has(size):
		var inner := size * PX
		var bank := PackedByteArray()
		bank.resize(inner * inner)
		bank.fill(255)
		var waters := PackedByteArray()
		waters.resize(size * size * 4)
		var still := int(round(0.5 * 255.0))
		for i in size * size:
			var o := 4 * i
			waters[o + 1] = still
			waters[o + 2] = still
			waters[o + 3] = 255
		_dry[size] = [
			Image.create_from_data(inner, inner, false, Image.FORMAT_R8, bank),
			Image.create_from_data(size, size, false, Image.FORMAT_RGBA8, waters),
		]
	return _dry[size]


## Lays `piece` (`side` pixels square) at `at` in the repeating `image`
## (`full` pixels square), in as many parts as it straddles its edges.
static func _blit(image: Image, piece: Image, at: Vector2i, side: int, full: int) -> void:
	var to := Vector2i(posmod(at.x, full), posmod(at.y, full))
	var y0 := 0
	while y0 < side:
		var rows := mini(side - y0, full - posmod(to.y + y0, full))
		var x0 := 0
		while x0 < side:
			var columns := mini(side - x0, full - posmod(to.x + x0, full))
			image.blit_rect(piece, Rect2i(x0, y0, columns, rows), Vector2i(posmod(to.x + x0, full), posmod(to.y + y0, full)))
			x0 += columns
		y0 += rows


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


## The weights of the convolution along one axis, by kind and chunk size:
## for each value sampled, [first tile, PackedFloat32Array of the weights
## of the tiles from it]. "bank": every pixel of the chunk, PX a tile, with
## the bank's kernel; "centre": the middle of every tile of the chunk with
## the same kernel (for the current); "ford": the same with the fords'
## kernel. Tiles past the block count as the nearest one in it, as the
## blurs' edges did.
static func _weights_of(kind: String, size: int) -> Array:
	var key := "%s %d" % [kind, size]
	if not _weights.has(key):
		var k := kernel(FORD_BLUR, FORD_BLUR_PASSES) if kind == "ford" else kernel(BLUR, BLUR_PASSES)
		var n := size + 2 * MARGIN
		var samples: Array = []
		if kind == "bank":
			for x in size * PX:
				samples.append([[MARGIN * PX + x, 1.0]])
		else:
			for t in size:
				var first := (MARGIN + t) * PX + (PX >> 1) - 1
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
					by_tile[clampi(floori(float(q) / PX), 0, n - 1)] += a * k[q - p + half]
			var lo := 0
			while lo < n - 1 and by_tile[lo] == 0.0:
				lo += 1
			var hi := n - 1
			while hi > lo and by_tile[hi] == 0.0:
				hi -= 1
			out.append([lo, by_tile.slice(lo, hi + 1)])
		_weights[key] = out
	return _weights[key]


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


## Smooths the masks of a block read with its margin (`n` tiles a side)
## and encodes the chunk's own tiles (`size` a side) into `out`: [0] the
## bank's distance, PX pixels a tile; [1] the fords and the flow, a pixel
## a tile. Touches nothing but its arguments, so it may run on another
## thread.
static func _smooth(out: Array, mask: PackedFloat32Array, fords: PackedFloat32Array, flow_x: PackedFloat32Array, flow_y: PackedFloat32Array, n: int, size: int, weights: Array, any_flow: bool, any_ford: bool, any_land: bool) -> void:
	var inner := size * PX
	var bank := PackedByteArray()
	bank.resize(inner * inner)
	if any_land:
		# A straight edge smoothed this way is the normal curve: undo it to
		# get a distance in tiles, sigma being the blur's spread.
		var field := convolve(mask, n, weights[0], weights[0])
		var sigma := sqrt(BLUR_PASSES * (pow(2.0 * BLUR + 1.0, 2.0) - 1.0) / 12.0) / PX
		for i in inner * inner:
			var d := -sigma * probit(clampf(field[i], 0.0005, 0.9995))
			bank[i] = int(round(clampf((d + RANGE) / (2.0 * RANGE), 0.0, 1.0) * 255.0))
	var waters := PackedByteArray()
	waters.resize(size * size * 4)
	var still := int(round(0.5 * 255.0))
	var near := PackedFloat32Array()
	var fx := PackedFloat32Array()
	var fy := PackedFloat32Array()
	if any_flow:
		near = convolve(mask, n, weights[1], weights[1])
		fx = convolve(flow_x, n, weights[1], weights[1])
		fy = convolve(flow_y, n, weights[1], weights[1])
	var ford := PackedFloat32Array()
	if any_ford:
		# Wider than the bank's smoothing, so a ford fades slowly into the
		# deep water.
		ford = convolve(fords, n, weights[2], weights[2])
	for i in size * size:
		var o := 4 * i
		if any_ford:
			waters[o] = int(round(clampf(ford[i], 0.0, 1.0) * 255.0))
		if any_flow and near[i] > 0.02:
			var v := Vector2(fx[i], fy[i]) / near[i]
			waters[o + 1] = int(round(clampf(v.x / FLOW_MAX * 0.5 + 0.5, 0.0, 1.0) * 255.0))
			waters[o + 2] = int(round(clampf(v.y / FLOW_MAX * 0.5 + 0.5, 0.0, 1.0) * 255.0))
		else:
			waters[o + 1] = still
			waters[o + 2] = still
		waters[o + 3] = 255
	out[0] = bank
	out[1] = waters


static func _upload() -> void:
	_upload_due = false
	_bank_texture.update(_bank_image)
	_water_texture.update(_water_image)


## The signed distance in tiles at a point of the map in tiles (a tile x
## spans [x, x + 1)), as the shader reads it; for the tests.
static func distance_at(p: Vector2) -> float:
	_make_textures()
	var px := Vector2i(posmod(floori(p.x * PX), TILES * PX), posmod(floori(p.y * PX), TILES * PX))
	return (_bank_image.get_pixelv(px).r * 2.0 - 1.0) * RANGE


## The water texture at a point of the map in tiles, filtered as the
## shader reads it (a pixel's value lies at its tile's middle).
static func _water_at(p: Vector2) -> Color:
	_make_textures()
	var q := p - Vector2(0.5, 0.5)
	var x0 := floori(q.x)
	var y0 := floori(q.y)
	var f := q - Vector2(x0, y0)
	var top := _water_pixel(x0, y0).lerp(_water_pixel(x0 + 1, y0), f.x)
	var bottom := _water_pixel(x0, y0 + 1).lerp(_water_pixel(x0 + 1, y0 + 1), f.x)
	return top.lerp(bottom, f.y)


static func _water_pixel(x: int, y: int) -> Color:
	return _water_image.get_pixel(posmod(x, TILES), posmod(y, TILES))


## Where the water runs at a point of the map in tiles, in metres a
## second, as the shader reads it.
static func flow_at(p: Vector2) -> Vector2:
	var c := _water_at(p)
	return Vector2(c.g * 2.0 - 1.0, c.b * 2.0 - 1.0) * FLOW_MAX


## How much of a ford there is at a point of the map in tiles, 0 to 1.
static func ford_at(p: Vector2) -> float:
	return _water_at(p).r


## The inverse of the normal curve (Abramowitz and Stegun 26.2.23, within
## 0.0005), so a smoothed edge reads as a distance.
static func probit(p: float) -> float:
	var q := p if p < 0.5 else 1.0 - p
	var t := sqrt(-2.0 * log(q))
	var x := t - (2.515517 + 0.802853 * t + 0.010328 * t * t) / (1.0 + 1.432788 * t + 0.189269 * t * t + 0.001308 * t * t * t)
	return -x if p < 0.5 else x


## A current as the protocol codes it, as a vector: its heading h (0 to
## 15) points along (sin(h × 22.5°), cos(h × 22.5°)), its length the speed
## in metres a second.
static func current_vector(code: int) -> Vector2:
	if code <= 0:
		return Vector2.ZERO
	var angle := deg_to_rad(float(code / 256) * 22.5)
	return Vector2(sin(angle), cos(angle)) * (float(code % 256) / 100.0)
