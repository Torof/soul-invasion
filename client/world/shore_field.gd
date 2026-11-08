class_name ShoreField
extends RefCounted
## The distance to the water's edge over the whole map, for the ground
## shader to draw the bank (D36): the tiles' water mask is smoothed, so
## the edge comes out round instead of in steps, and turned into a signed
## distance in tiles, positive on the land. One shared texture, PX pixels
## a tile; each chunk writes its own block when it is drawn, read with a
## margin of its neighbours so the blocks join. A second channel carries
## the fords, smoothed the same way: 1 inside a ford, 0 away from one.

## Pixels a tile.
const PX := 4
## Tiles of the map the texture covers, from (0, 0).
const TILES := 256
## Distances kept, either way, in tiles.
const RANGE := 4.0
## Box blur radius in pixels, run three times: about 0.9 tile of
## smoothing, enough to round the steps of the tiles away.
const BLUR := 3
## Tiles read around a chunk.
const MARGIN := 5
## Blur for the fords' channel: wider boxes, fewer passes, about 2.4
## tiles of spread.
const FORD_BLUR := 7
const FORD_BLUR_PASSES := 5

## The map's size in tiles: past it the server sends rock, which here
## counts as unknown so a river runs on out of the map.
static var map_size := Vector2i(TILES, TILES)

static var _image: Image
static var _texture: ImageTexture


static func texture() -> ImageTexture:
	if _texture == null:
		_image = Image.create(TILES * PX, TILES * PX, false, Image.FORMAT_RG8)
		_image.fill(Color(1, 0, 0))  # all land, far from water, no ford
		_texture = ImageTexture.create_from_image(_image)
	return _texture


## Is the tile water for the bank: a river or a ford; "" (unknown) and
## everything else is land.
static func is_water(terrain: String) -> bool:
	return terrain == "water" or terrain == "ford"


## Writes the block of tiles [lo, lo + size) from the terrains `terrain`
## gives (a Callable of Vector2i -> String).
static func update(terrain: Callable, lo: Vector2i, size: int) -> void:
	texture()
	var w := (size + 2 * MARGIN) * PX
	var start := lo - Vector2i(MARGIN, MARGIN)
	var field := PackedFloat32Array()
	field.resize(w * w)
	var fords := PackedFloat32Array()
	fords.resize(w * w)
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
			if water:
				var ford := t == "ford"
				any_ford = any_ford or ford
				for py in PX:
					var row := (ty * PX + py) * w + tx * PX
					for px in PX:
						field[row + px] = 1.0
						if ford:
							fords[row + px] = 1.0
	var inner := size * PX
	var block := PackedByteArray()
	block.resize(inner * inner * 2)
	if any_ford:
		# Wider than the bank's smoothing, so a ford fades slowly into the
		# deep water.
		for _pass in FORD_BLUR_PASSES:
			_blur_rows(fords, w, FORD_BLUR)
			_blur_columns(fords, w, FORD_BLUR)
	if not any_water or not any_land:
		var far := 255 if not any_water else 0
		for i in inner * inner:
			block[2 * i] = far
	else:
		for _pass in 3:
			_blur_rows(field, w)
			_blur_columns(field, w)
		# A straight edge smoothed this way is the normal curve: undo it to
		# get a distance in tiles, sigma being the blur's spread.
		var sigma := sqrt(3.0 * (pow(2.0 * BLUR + 1.0, 2.0) - 1.0) / 12.0) / PX
		for y in inner:
			for x in inner:
				var v := field[(y + MARGIN * PX) * w + x + MARGIN * PX]
				var d := -sigma * probit(clampf(v, 0.0005, 0.9995))
				block[2 * (y * inner + x)] = int(round(clampf((d + RANGE) / (2.0 * RANGE), 0.0, 1.0) * 255.0))
	if any_ford:
		for y in inner:
			for x in inner:
				block[2 * (y * inner + x) + 1] = int(round(clampf(fords[(y + MARGIN * PX) * w + x + MARGIN * PX], 0.0, 1.0) * 255.0))
	var piece := Image.create_from_data(size * PX, size * PX, false, Image.FORMAT_RG8, block)
	var at := lo * PX
	var src := Rect2i(Vector2i.ZERO, Vector2i(size * PX, size * PX))
	# Clip to the map.
	var clipped := Rect2i(at, src.size).intersection(Rect2i(0, 0, TILES * PX, TILES * PX))
	if clipped.size.x <= 0 or clipped.size.y <= 0:
		return
	_image.blit_rect(piece, Rect2i(clipped.position - at, clipped.size), clipped.position)
	_texture.update(_image)


## The signed distance in tiles at a point of the map in tiles (a tile x
## spans [x, x + 1)), as the shader reads it; for the tests.
static func distance_at(p: Vector2) -> float:
	texture()
	var px := Vector2i(clampi(int(p.x * PX), 0, TILES * PX - 1), clampi(int(p.y * PX), 0, TILES * PX - 1))
	return (_image.get_pixelv(px).r * 2.0 - 1.0) * RANGE


## How much of a ford there is at a point of the map in tiles, 0 to 1.
static func ford_at(p: Vector2) -> float:
	texture()
	var px := Vector2i(clampi(int(p.x * PX), 0, TILES * PX - 1), clampi(int(p.y * PX), 0, TILES * PX - 1))
	return _image.get_pixelv(px).g


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
