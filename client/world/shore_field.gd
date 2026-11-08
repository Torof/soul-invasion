class_name ShoreField
extends RefCounted
## The distance to the water's edge around the view, for the ground
## shader to draw the bank (D36): the tiles' water mask is smoothed, so
## the edge comes out round instead of in steps, and turned into a signed
## distance in tiles, positive on the land. One shared texture, PX pixels
## a tile, of TILES × TILES tiles laid over the map again and again: the
## tile (x, y) is at (x mod TILES, y mod TILES), so the texture does not
## grow with the map, and neighbours on the map are neighbours in it.
## TILES is wider than the chunks a client holds, so two of them never
## share a place. Each chunk writes its own block when it is drawn, read
## with a margin of its neighbours so the blocks join. A second channel carries
## the fords, smoothed the same way: 1 inside a ford, 0 away from one. The
## last two carry where the water runs (from the server's currents),
## averaged over the water nearby: x then y, in metres a second, over
## ±FLOW_MAX, 0.5 for still.

## Pixels a tile.
const PX := 4
## Tiles the texture covers either way, then repeats: twelve chunks, more
## than the nine a client holds across (the server's view window).
const TILES := 384
## Distances kept, either way, in tiles.
const RANGE := 4.0
## Box blur radius in pixels, run three times: about 0.9 tile of
## smoothing, enough to round the steps of the tiles away.
const BLUR := 3
## Fastest current the field carries, in metres a second.
const FLOW_MAX := 2.0
## Tiles read around a chunk.
const MARGIN := 5
## Blur for the fords' channel: wider boxes, fewer passes, about 2.4
## tiles of spread.
const FORD_BLUR := 7
const FORD_BLUR_PASSES := 5

## The map's size in tiles: past it the server sends rock, which here
## counts as unknown so a river runs on out of the map.
static var map_size := Vector2i(256, 256)

## The latest update asked for each block: an update in slices gives up
## when a newer one for its block has started.
static var _jobs: Dictionary = {}
## Updates under way, in slices or not (the loading screen waits for them).
static var busy := 0
## The frame the slices are running in, and when they started in it.
static var _slice_frame := -1
static var _slice_start := 0
static var _image: Image
static var _texture: ImageTexture
## Whether the image changed since it was last sent to the texture: sent
## once a frame at most, however many blocks were written.
static var _upload_due := false


static func texture() -> ImageTexture:
	if _texture == null:
		_image = Image.create(TILES * PX, TILES * PX, false, Image.FORMAT_RGBA8)
		_image.fill(Color(1, 0, 0.5, 0.5))  # all land, no ford, still
		_texture = ImageTexture.create_from_image(_image)
	return _texture


## Is the tile water for the bank: a river or a ford; "" (unknown) and
## everything else is land.
static func is_water(terrain: String) -> bool:
	return terrain == "water" or terrain == "ford"


## Writes the block of tiles [lo, lo + size) from the terrains `terrain`
## gives (a Callable of Vector2i -> String).
## `current`, when valid, gives where the water runs at a tile (a vector
## in metres a second, Vector2i -> Vector2).
## With `slice_usec`, the smoothing is spread over frames, that long at
## most at a time (a block takes tens of milliseconds): the tiles are read
## at once, the texture is written a few frames later.
static func update(terrain: Callable, lo: Vector2i, size: int, current := Callable(), slice_usec := 0) -> void:
	busy += 1
	await _update(terrain, lo, size, current, slice_usec)
	busy -= 1


static func _update(terrain: Callable, lo: Vector2i, size: int, current: Callable, slice_usec: int) -> void:
	texture()
	var job := int(_jobs.get(lo, 0)) + 1
	_jobs[lo] = job
	var w := (size + 2 * MARGIN) * PX
	var start := lo - Vector2i(MARGIN, MARGIN)
	var field := PackedFloat32Array()
	field.resize(w * w)
	var fords := PackedFloat32Array()
	fords.resize(w * w)
	var flow_x := PackedFloat32Array()
	flow_x.resize(w * w)
	var flow_y := PackedFloat32Array()
	flow_y.resize(w * w)
	var any_flow := false
	var any_ford := false
	var any_water := false
	var any_land := false
	for ty in size + 2 * MARGIN:
		for tx in size + 2 * MARGIN:
			var at := start + Vector2i(tx, ty)
			var inside := at.x >= 0 and at.y >= 0 and at.x < map_size.x and at.y < map_size.y
			var t := str(terrain.call(at)) if inside else ""
			if t == "":
				# Unknown (past the map's edge, or not sent yet): the nearest
				# tile of the chunk goes on, so a river runs on out of view
				# instead of being closed by a bank.
				var near := Vector2i(clampi(at.x, lo.x, lo.x + size - 1), clampi(at.y, lo.y, lo.y + size - 1))
				near = Vector2i(clampi(near.x, 0, map_size.x - 1), clampi(near.y, 0, map_size.y - 1))
				t = str(terrain.call(near))
			var water := is_water(t)
			any_water = any_water or water
			any_land = any_land or not water
			var runs := Vector2.ZERO
			if water and current.is_valid():
				runs = current.call(at) if inside else Vector2.ZERO
				any_flow = any_flow or runs != Vector2.ZERO
			if water:
				var ford := t == "ford"
				any_ford = any_ford or ford
				for py in PX:
					var row := (ty * PX + py) * w + tx * PX
					for px in PX:
						field[row + px] = 1.0
						flow_x[row + px] = runs.x
						flow_y[row + px] = runs.y
						if ford:
							fords[row + px] = 1.0
	var inner := size * PX
	var block := PackedByteArray()
	block.resize(inner * inner * 4)
	# The water's mask smoothed: the bank's distance, and what the flow is
	# averaged over.
	if any_flow or (any_water and any_land):
		for _pass in 3:
			_blur_rows(field, w)
			_blur_columns(field, w)
			if not await _breathe(lo, job, slice_usec):
				return
	if any_flow:
		for _pass in 3:
			_blur_rows(flow_x, w)
			_blur_columns(flow_x, w)
			_blur_rows(flow_y, w)
			_blur_columns(flow_y, w)
			if not await _breathe(lo, job, slice_usec):
				return
	if any_ford:
		# Wider than the bank's smoothing, so a ford fades slowly into the
		# deep water.
		for _pass in FORD_BLUR_PASSES:
			_blur_rows(fords, w, FORD_BLUR)
			_blur_columns(fords, w, FORD_BLUR)
			if not await _breathe(lo, job, slice_usec):
				return
	if not any_water or not any_land:
		var far := 255 if not any_water else 0
		for i in inner * inner:
			block[4 * i] = far
	else:
		# A straight edge smoothed this way is the normal curve: undo it to
		# get a distance in tiles, sigma being the blur's spread.
		var sigma := sqrt(3.0 * (pow(2.0 * BLUR + 1.0, 2.0) - 1.0) / 12.0) / PX
		for y in inner:
			for x in inner:
				var v := field[(y + MARGIN * PX) * w + x + MARGIN * PX]
				var d := -sigma * probit(clampf(v, 0.0005, 0.9995))
				block[4 * (y * inner + x)] = int(round(clampf((d + RANGE) / (2.0 * RANGE), 0.0, 1.0) * 255.0))
	if any_ford:
		for y in inner:
			for x in inner:
				block[4 * (y * inner + x) + 1] = int(round(clampf(fords[(y + MARGIN * PX) * w + x + MARGIN * PX], 0.0, 1.0) * 255.0))
	var still := int(round(0.5 * 255.0))
	for y in inner:
		for x in inner:
			var o := 4 * (y * inner + x)
			var i := (y + MARGIN * PX) * w + x + MARGIN * PX
			if any_flow and field[i] > 0.02:
				var v := Vector2(flow_x[i], flow_y[i]) / field[i]
				block[o + 2] = int(round(clampf(v.x / FLOW_MAX * 0.5 + 0.5, 0.0, 1.0) * 255.0))
				block[o + 3] = int(round(clampf(v.y / FLOW_MAX * 0.5 + 0.5, 0.0, 1.0) * 255.0))
			else:
				block[o + 2] = still
				block[o + 3] = still
	var piece := Image.create_from_data(size * PX, size * PX, false, Image.FORMAT_RGBA8, block)
	# Laid at its place in the repeating texture, in as many parts as it
	# straddles the texture's edges.
	var at := Vector2i(posmod(lo.x, TILES), posmod(lo.y, TILES)) * PX
	var full := TILES * PX
	var y0 := 0
	while y0 < size * PX:
		var rows := mini(size * PX - y0, full - posmod(at.y + y0, full))
		var x0 := 0
		while x0 < size * PX:
			var columns := mini(size * PX - x0, full - posmod(at.x + x0, full))
			_image.blit_rect(piece, Rect2i(x0, y0, columns, rows), Vector2i(posmod(at.x + x0, full), posmod(at.y + y0, full)))
			x0 += columns
		y0 += rows
	if not _upload_due:
		_upload_due = true
		_upload.call_deferred()


static func _upload() -> void:
	_upload_due = false
	_texture.update(_image)


## The signed distance in tiles at a point of the map in tiles (a tile x
## spans [x, x + 1)), as the shader reads it; for the tests.
static func distance_at(p: Vector2) -> float:
	texture()
	var px := Vector2i(posmod(floori(p.x * PX), TILES * PX), posmod(floori(p.y * PX), TILES * PX))
	return (_image.get_pixelv(px).r * 2.0 - 1.0) * RANGE


## Where the water runs at a point of the map in tiles, in metres a
## second, as the shader reads it.
static func flow_at(p: Vector2) -> Vector2:
	texture()
	var px := Vector2i(posmod(floori(p.x * PX), TILES * PX), posmod(floori(p.y * PX), TILES * PX))
	var c := _image.get_pixelv(px)
	return Vector2(c.b * 2.0 - 1.0, c.a * 2.0 - 1.0) * FLOW_MAX


## How much of a ford there is at a point of the map in tiles, 0 to 1.
static func ford_at(p: Vector2) -> float:
	texture()
	var px := Vector2i(posmod(floori(p.x * PX), TILES * PX), posmod(floori(p.y * PX), TILES * PX))
	return _image.get_pixelv(px).g


## Between two slices of an update: once the updates in slices have used
## their time in this frame, all of them together, waits for a frame with
## time left; tells whether the update should go on.
static func _breathe(lo: Vector2i, job: int, slice_usec: int) -> bool:
	if slice_usec <= 0:
		return true
	var tree := Engine.get_main_loop() as SceneTree
	if tree == null:
		return true
	while true:
		var frame := Engine.get_process_frames()
		if frame != _slice_frame:
			_slice_frame = frame
			_slice_start = Time.get_ticks_usec()
		if Time.get_ticks_usec() - _slice_start < slice_usec:
			break
		await tree.process_frame
	return _jobs.get(lo, 0) == job


static func _blur_rows(field: PackedFloat32Array, w: int, radius := BLUR) -> void:
	var line := PackedFloat32Array()
	line.resize(w)
	var n := 2 * radius + 1
	for y in w:
		var row := y * w
		var sum := 0.0
		for k in range(-radius, radius + 1):
			sum += field[row + clampi(k, 0, w - 1)]
		for x in w:
			line[x] = sum / n
			sum += field[row + mini(x + radius + 1, w - 1)] - field[row + maxi(x - radius, 0)]
		for x in w:
			field[row + x] = line[x]


static func _blur_columns(field: PackedFloat32Array, w: int, radius := BLUR) -> void:
	var line := PackedFloat32Array()
	line.resize(w)
	var n := 2 * radius + 1
	for x in w:
		var sum := 0.0
		for k in range(-radius, radius + 1):
			sum += field[clampi(k, 0, w - 1) * w + x]
		for y in w:
			line[y] = sum / n
			sum += field[mini(y + radius + 1, w - 1) * w + x] - field[maxi(y - radius, 0) * w + x]
		for y in w:
			field[y * w + x] = line[y]


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
