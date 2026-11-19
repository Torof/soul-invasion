class_name HeightIndex
extends RefCounted
## The ground's height under each tile, as the chunks say it (protocol
## v25, D43): one height a tile in decimetres above the sea, kept in a
## ring of TILES × TILES laid over the map again and again, as
## GroundIndex and TerrainGrid are. A chunk far enough away lands on the
## same cells as one in view and overwrites it, which is the ring's
## point: only what is around the body is kept.
## From the heights, the slopes (D44): a texture of TILES × TILES for the
## ground shader, R and G (half floats) the ground's rise along the map's
## x and y, in metres a tile, read over SPAN tiles on either side (a slope
## is read over about two tiles, not tile to tile: the project lead's
## eye, 13 November 2026). Where a neighbour is not known yet, the
## nearest known tile toward it stands in, and the tile itself at worst.
## Only the test worlds (`--world test:*`) send heights today; the
## valley and the game's worlds send none and never write here, and
## their slopes stay off.

const TILES := GroundIndex.TILES
## How far on either side the slope is read, in tiles.
const SPAN := 2

## The heights, in decimetres, and whether a chunk said them.
static var _heights := PackedInt32Array()
static var _known := PackedByteArray()
## The slope texture's image (two half floats a tile), the texture the
## ground shader reads; the upload once a frame at most.
static var _image: Image
static var _texture: ImageTexture
static var _upload_due := false
## Whether any chunk carried heights.
static var any := false
## The slopes of the chunks written, worked out on the workers (step 4 of
## the lightening, 19 November 2026: 10.8 ms a chunk on the main thread
## before, 3.9 s of an arrival) and laid in the order the chunks came, so
## the image is, job after job, what working them out in line gave: each
## job reads the heights as they were when its chunk came (a copy, shared
## until the main thread writes the next chunk's), and a later chunk's job
## lays its slopes over an earlier one's. A job: [ticket, task, the
## region's first tile, its side, its result (an image of the region)].
static var _jobs: Array = []
## Tickets given (one a write) and laid.
static var _given := 0
static var _laid := 0


static func _ready_store() -> void:
	if _heights.is_empty():
		_heights.resize(TILES * TILES)
		_known.resize(TILES * TILES)
		_image = Image.create_empty(TILES, TILES, false, Image.FORMAT_RGH)


## The slope texture (R, G: the rise along x and y in metres a tile), the
## same object for the life of the client, updated in place.
static func texture() -> ImageTexture:
	_ready_store()
	if _texture == null:
		_texture = ImageTexture.create_from_image(_image)
	return _texture


static func _index(x: int, y: int) -> int:
	return posmod(y, TILES) * TILES + posmod(x, TILES)


## Lays a chunk's heights (`size` × `size`, decimetres, row by row from
## its north-west corner) at `lo`, the chunk's first tile, and gives a
## worker the slopes of its tiles and of the tiles within SPAN of it, to
## work out again (laid by `lay_ready`, `lay_through` or `settle`).
static func write(lo: Vector2i, size: int, heights: PackedInt32Array) -> void:
	if heights.size() != size * size:
		return
	_ready_store()
	for y in size:
		var row := posmod(lo.y + y, TILES) * TILES
		for x in size:
			var i := row + posmod(lo.x + x, TILES)
			_heights[i] = heights[y * size + x]
			_known[i] = 1
	any = true
	var from := lo - Vector2i(SPAN, SPAN)
	var side := size + 2 * SPAN
	var out: Array = []
	_given += 1
	var task := WorkerThreadPool.add_task(_work.bind(_heights, _known, from, side, out), true, "chunk slopes")
	_jobs.append([_given, task, from, side, out])


## The ticket of the last chunk written: `lay_through` it, and its slopes
## are on the image.
static func ticket() -> int:
	return _given


## On a worker: the slopes of the `side` × `side` tiles from `from`, as
## an image, from the heights as they were (`heights`, `known`, copies the
## main thread no longer writes). A tile no chunk said gets 0, as its texel
## holds (a tile once said is never unsaid but by `clear`).
static func _work(heights: PackedInt32Array, known: PackedByteArray, from: Vector2i, side: int, out: Array) -> void:
	out.append(slopes_of(heights, known, from, side))


## The slopes' image of the `side` × `side` tiles from `from`.
static func slopes_of(heights: PackedInt32Array, known: PackedByteArray, from: Vector2i, side: int) -> Image:
	var bytes := PackedByteArray()
	bytes.resize(side * side * 4)
	for y in side:
		for x in side:
			var tile := Vector2i(from.x + x, from.y + y)
			if known[_index(tile.x, tile.y)] == 0:
				continue
			var o := 4 * (y * side + x)
			bytes.encode_half(o, _rise_in(heights, known, tile, Vector2i(1, 0)))
			bytes.encode_half(o + 2, _rise_in(heights, known, tile, Vector2i(0, 1)))
	return Image.create_from_data(side, side, false, Image.FORMAT_RGH, bytes)


## Lays the slopes the workers are done with, in the order the chunks
## came, stopping at the first not done. How many were laid.
static func lay_ready() -> int:
	var n := 0
	while not _jobs.is_empty() and WorkerThreadPool.is_task_completed(int(_jobs[0][1])):
		_lay_first()
		n += 1
	return n


## Lays the slopes of every chunk written up to `up_to` (`ticket`),
## waiting for their workers: a chunk's ground is laid with its slopes.
static func lay_through(up_to: int) -> void:
	while not _jobs.is_empty() and int(_jobs[0][0]) <= up_to:
		_lay_first()


## Lays every slope under way, waiting for the workers (the tests, and
## the game quitting: no worker is left in a script). How many.
static func settle() -> int:
	var n := _jobs.size()
	lay_through(_given)
	return n


## The first job, waited for and laid: its region blitted on the image
## (cut where it crosses the ring's edges), the upload due.
static func _lay_first() -> void:
	var job: Array = _jobs.pop_front()
	WorkerThreadPool.wait_for_task_completion(int(job[1]))
	_laid = int(job[0])
	var out: Array = job[4]
	if out.is_empty() or _image == null:
		return
	var region: Image = out[0]
	var from: Vector2i = job[2]
	var side: int = job[3]
	var x0 := posmod(from.x, TILES)
	var y0 := posmod(from.y, TILES)
	var wx := mini(side, TILES - x0)
	var wy := mini(side, TILES - y0)
	_image.blit_rect(region, Rect2i(0, 0, wx, wy), Vector2i(x0, y0))
	if wx < side:
		_image.blit_rect(region, Rect2i(wx, 0, side - wx, wy), Vector2i(0, y0))
	if wy < side:
		_image.blit_rect(region, Rect2i(0, wy, wx, side - wy), Vector2i(x0, 0))
		if wx < side:
			_image.blit_rect(region, Rect2i(wx, wy, side - wx, side - wy), Vector2i(0, 0))
	if _texture != null and not _upload_due:
		_upload_due = true
		_upload.call_deferred()


## How many chunks' slopes are still under way.
static func pending() -> int:
	return _jobs.size()


## The rise at `tile` along `axis` (a unit step along x or y), in metres
## a tile: the difference of the farthest known tiles within SPAN on
## either side, over their distance, the tile itself standing in for a
## side with no known tile; 0 where the tile itself is unknown, as its
## texel is.
static func rise(tile: Vector2i, axis: Vector2i) -> float:
	_ready_store()
	return _rise_in(_heights, _known, tile, axis)


static func _rise_in(heights: PackedInt32Array, known: PackedByteArray, tile: Vector2i, axis: Vector2i) -> float:
	if known[_index(tile.x, tile.y)] == 0:
		return 0.0
	var ahead := 0
	for k in range(SPAN, 0, -1):
		if known[_index(tile.x + k * axis.x, tile.y + k * axis.y)] != 0:
			ahead = k
			break
	var behind := 0
	for k in range(SPAN, 0, -1):
		if known[_index(tile.x - k * axis.x, tile.y - k * axis.y)] != 0:
			behind = k
			break
	if ahead + behind == 0:
		return 0.0
	var high := heights[_index(tile.x + ahead * axis.x, tile.y + ahead * axis.y)]
	var low := heights[_index(tile.x - behind * axis.x, tile.y - behind * axis.y)]
	return (high - low) / 10.0 / (ahead + behind)


static func _upload() -> void:
	_upload_due = false
	if _texture == null:
		return
	_texture.update(_image)


## The slope texture's bytes as laid (the tests).
static func bytes() -> PackedByteArray:
	_ready_store()
	return _image.get_data()


## Whether a chunk said the height of `tile`.
static func known(tile: Vector2i) -> bool:
	_ready_store()
	return _known[_index(tile.x, tile.y)] != 0


## A tile's height in decimetres above the sea, 0 where no chunk said it
## (`known` tells the two apart).
static func height_dm(tile: Vector2i) -> int:
	_ready_store()
	return _heights[_index(tile.x, tile.y)]


## A tile's height in metres, 0.0 where no chunk said it.
static func level(tile: Vector2i) -> float:
	return height_dm(tile) / 10.0


## Forgets every height and every slope (the tests), the slopes under
## way first; the texture, if made, is emptied in place.
static func clear() -> void:
	settle()
	_heights = PackedInt32Array()
	_known = PackedByteArray()
	any = false
	_ready_store()
	_upload()
