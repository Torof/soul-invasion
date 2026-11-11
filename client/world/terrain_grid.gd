class_name TerrainGrid
extends RefCounted
## The terrain of every tile the client holds, one byte a tile, in one
## grid of TILES × TILES laid over the map again and again: tile (x, y)
## is at (x mod TILES, y mod TILES), so the grid never grows with the map
## and a chunk reads its neighbours without asking the world view. TILES
## is wider than the chunks held plus the widest margin read around a
## chunk, so two tiles read together never share a place (D39: the
## ground is built from bytes, not strings).
##
## A chunk's frame carries its tiles in these codes (protocol v17), laid
## as they come; a terrain named in text is turned into its code once. 0
## is "not known" (not sent, or hidden since).

## Tiles the grid covers either way, then repeats (wider than the chunks a client holds).
const TILES := 640
## Codes of the terrains the client knows by name. A name the server adds
## later gets the next free code on first sight.
const UNKNOWN := 0
const GRASS := 1
const FOREST := 2
const WATER := 3
const FORD := 4
const BROOK := 5
const ROCK := 6
const _FIRST_NAMES := ["", "grass", "forest", "water", "ford", "brook", "rock"]

static var _names: PackedStringArray = PackedStringArray(_FIRST_NAMES)
static var _codes: Dictionary = {}
static var _grid := PackedByteArray()
## The chunk (its north-west tile) that last wrote each block of
## CHUNK × CHUNK tiles, so hiding a chunk never clears a newer one laid
## at the same place of the grid.
static var _owners: Dictionary = {}


## The code of a terrain name ("" is unknown).
static func code_of(terrain: String) -> int:
	if _codes.is_empty():
		for i in _names.size():
			_codes[_names[i]] = i
	var code: Variant = _codes.get(terrain)
	if code == null:
		if _names.size() >= 256:
			return UNKNOWN
		code = _names.size()
		_names.append(terrain)
		_codes[terrain] = code
	return int(code)


## The name of a code, "" when unknown.
static func name_of(code: int) -> String:
	return _names[code] if code >= 0 and code < _names.size() else ""


## The tiles as received (names) or already as codes, as codes.
static func codes(tiles: Variant) -> PackedByteArray:
	if tiles is PackedByteArray:
		return tiles
	var out := PackedByteArray()
	var names: Array = tiles
	out.resize(names.size())
	for i in names.size():
		out[i] = code_of(str(names[i]))
	return out


static func _ready_grid() -> void:
	if _grid.is_empty():
		_grid.resize(TILES * TILES)


## Lays the codes of the block of `size` × `size` tiles at `lo`.
static func write(lo: Vector2i, size: int, block: PackedByteArray) -> void:
	_ready_grid()
	for y in size:
		var row := posmod(lo.y + y, TILES) * TILES
		for x in size:
			_grid[row + posmod(lo.x + x, TILES)] = block[y * size + x]
	_owners[_slot(lo)] = lo


## Forgets the block of `size` × `size` tiles at `lo` (a chunk hidden),
## unless a newer chunk has been laid at its place since.
static func clear(lo: Vector2i, size: int) -> void:
	_ready_grid()
	if _owners.get(_slot(lo), lo) != lo:
		return
	_owners.erase(_slot(lo))
	for y in size:
		var row := posmod(lo.y + y, TILES) * TILES
		for x in size:
			_grid[row + posmod(lo.x + x, TILES)] = UNKNOWN


## Forgets everything (a new world).
static func clear_all() -> void:
	_grid = PackedByteArray()
	_owners.clear()


static func _slot(lo: Vector2i) -> Vector2i:
	return Vector2i(posmod(lo.x, TILES), posmod(lo.y, TILES))


## The code of one tile.
static func at(tile: Vector2i) -> int:
	_ready_grid()
	return _grid[posmod(tile.y, TILES) * TILES + posmod(tile.x, TILES)]


## The name of one tile's terrain, "" when unknown.
static func name_at(tile: Vector2i) -> String:
	return name_of(at(tile))


## The codes of the `w` × `h` tiles from `lo`, row after row: what a
## chunk reads once, with its margin, before building its ground.
static func window(lo: Vector2i, w: int, h: int) -> PackedByteArray:
	_ready_grid()
	var out := PackedByteArray()
	var x0 := posmod(lo.x, TILES)
	var first := mini(w, TILES - x0)
	for y in h:
		var row := posmod(lo.y + y, TILES) * TILES
		out.append_array(_grid.slice(row + x0, row + x0 + first))
		if first < w:
			out.append_array(_grid.slice(row, row + w - first))
	return out
