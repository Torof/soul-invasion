class_name GroundBuild
extends RefCounted
## The heavy part of a chunk's ground as pure data, worked out on a worker
## thread (C3 of the lightness plan, 11 November 2026): which canvas draws
## each tile and the forest's spill (the ground index's bytes), the flat
## tiles, the decorations, the water meshes' arrays (WaterMeshes.prepare
## and build) and the brooks' ribbons (BrookLines). The main thread
## gathers what it reads (`gather` in ChunkView: the codes around the
## chunk, the currents, which terrains have a texture, the decorations'
## rules and images) and lays the result (ChunkView._lay): it only writes
## the index, makes the meshes from the arrays and issues the canvas
## commands. Before, all of it ran on the main thread, 45 to 185 ms on the
## frame a chunk came into the camera's reach.
##
## Touches nothing but its input and static tables made beforehand on the
## main thread (WaterMeshes.warm), so it may run on any thread; run in line
## it gives the same result, bit for bit.

const READ := WaterMeshes.READ
## How far around the chunk the brooks are read: their window
## (BrookLines.MARGIN) and the tile around its ends.
const BROOK_READ := BrookLines.MARGIN + 2


## Runs `compute(input)` and puts its result in `out` (a worker's task).
static func run(input: Dictionary, out: Array) -> void:
	out.append(compute(input))


## The chunk's ground from `input` (ChunkView._gather): "flats" [points,
## colour], "decor" [texture, rect, tint], "bits" and "corners" (the
## ground index's bytes, GroundIndex.write), "used" (the canvas bits
## used), "water" and "bank" (WaterMeshes.build's arrays, empty when
## none), "water_usec" (its time, -1 when no water), "brooks" (the
## ribbons' arrays, or null without a brook), "brook_lines" (the lines,
## drawn flat when the brook's art is missing), "arrows", "plant_draws"
## (the plants' layer, ChunkView.PlantLayer.draws_of; null when none was
## asked for).
static func compute(input: Dictionary) -> Dictionary:
	var origin: Vector2i = input["origin"]
	var size: int = input["size"]
	var win: PackedByteArray = input["win"]
	var codes: PackedByteArray = input["codes"]
	var bit_of: PackedByteArray = input["bit_of"]
	var colours: Array = input["colours"]
	# The terrain drawn on each tile, then its canvas's bit (0: flat); the
	# water's tiles are the meshes' (the land beneath where the bank can
	# show is added below).
	var flats: Array = []
	var drawn := PackedByteArray()
	drawn.resize(size * size)
	var bits := PackedByteArray()
	bits.resize(size * size)
	for y in size:
		for x in size:
			var code := drawn_code(win, x + READ, y + READ, size)
			drawn[y * size + x] = code
			if code == TerrainGrid.WATER:
				continue
			if bit_of[code] != 0:
				bits[y * size + x] = bit_of[code]
			else:
				flats.append([Iso.diamond(origin.x + x, origin.y + y), colours[code]])
	# A terrain that spills (the forest) is drawn over the tiles it spills
	# on, its opacity at their corners in the index.
	var corners := PackedByteArray()
	corners.resize((size + 1) * (size + 1))
	for spill: Array in input["fringe"]:
		var over: PackedByteArray = spill[1]
		var nearness := nearness_of(win, table([int(spill[0])]), int(spill[2]), size)
		for i in corners.size():
			corners[i] = int(round(nearness[i] * 255.0))
		for y in size:
			for x in size:
				if not over[drawn[y * size + x]]:
					continue
				var alphas := corners_of(nearness, x, y, size)
				if alphas[0] > 0.0 or alphas[1] > 0.0 or alphas[2] > 0.0 or alphas[3] > 0.0:
					bits[y * size + x] |= GroundIndex.FRINGE
	# Under the water meshes, the land beneath where the bank can show:
	# the smoothed line can fall anywhere across the tiles' edge, so the
	# water tiles touching land draw the land around them (forest if the
	# land around is mostly forest, else grass) under the bank mesh.
	var side := size + 2 * READ
	for y in size:
		for x in size:
			if WaterMeshes.is_water(win[(y + READ) * side + x + READ]):
				bits[y * size + x] |= bit_of[land_around(win, x + READ, y + READ, true, size)]
	var used := 0
	for b in bits:
		used |= b
	var current: Callable = input["current"]
	var result := {"flats": flats, "bits": bits, "corners": corners, "used": used,
		"water": {}, "bank": {}, "water_usec": -1, "brooks": null, "brook_lines": [], "arrows": []}
	var prepared := WaterMeshes.prepare(win, origin, size, current)
	if not prepared.is_empty():
		var out: Array = []
		WaterMeshes.build(prepared, out)
		result["water"] = out[0]
		result["bank"] = out[1]
		result["water_usec"] = int(out[4])
	_brooks(input, prepared, current, result)
	var plants: Array = input["plants"]
	result["plant_draws"] = ChunkView.PlantLayer.draws_of(plants[0], plants[1]) if not plants.is_empty() else null
	result["decor"] = _decor(input, win, codes)
	return result


## The brooks crossing the chunk: a mesh of ribbons (BrookLines.mesh),
## each vertex with its own tile's current and, at a mouth, the fade into
## the river's water read from the river's line (`prepared`, empty when no
## river is near); without the brook's art, the lines to draw flat.
static func _brooks(input: Dictionary, prepared: Dictionary, current: Callable, result: Dictionary) -> void:
	var win: PackedByteArray = input["win"]
	if not win.has(TerrainGrid.BROOK):
		return
	var origin: Vector2i = input["origin"]
	var size: int = input["size"]
	var brook_win: PackedByteArray = input["brook_win"]
	var brook_lo := origin - Vector2i(BROOK_READ, BROOK_READ)
	var brook_side := size + 2 * BROOK_READ
	var terrain := func(t: Vector2i) -> String:
		var at := t - brook_lo
		if at.x < 0 or at.y < 0 or at.x >= brook_side or at.y >= brook_side:
			return TerrainGrid.name_at(t)
		return TerrainGrid.name_of(brook_win[at.y * brook_side + at.x])
	var margin := Vector2i(BrookLines.MARGIN, BrookLines.MARGIN)
	var hi := origin + Vector2i(size, size)
	var lines := BrookLines.lay(terrain, origin - margin, hi + margin)
	if not bool(input["brook_bed"]):
		result["brook_lines"] = lines
		return
	var to_river := Callable()
	if not prepared.is_empty():
		to_river = func(p: Vector2) -> float: return WaterMeshes.distance_at(prepared, p)
	result["brooks"] = BrookLines.mesh(lines, origin, hi, current, to_river)
	if bool(input["arrows"]):
		var arrows: Array = []
		for line: Dictionary in lines:
			var pts: PackedVector2Array = line["pts"]
			var flows := BrookLines.flows_of(line, current)
			for i in pts.size() - 1:
				if BrookLines.owns(line, i, origin, hi):
					arrows.append([BrookLines.to_screen(pts[i]), flows[i]])
		result["arrows"] = arrows


## Sows the decorations of the chunk: for each kind (`input["decor"]`,
## gathered with their images), on the tiles its "where" names (water
## within a tile of land; land within two tiles of water), with its
## chance, at a fixed random spot of the tile. [texture, rect, tint].
static func _decor(input: Dictionary, win: PackedByteArray, codes: PackedByteArray) -> Array:
	var kinds: Array = input["decor"]
	var out: Array = []
	if kinds.is_empty():
		return out
	var origin: Vector2i = input["origin"]
	var size: int = input["size"]
	var by_water_wanted := false
	var by_land_wanted := false
	for rule: Dictionary in kinds:
		if rule["by_water"]:
			by_water_wanted = true
		else:
			by_land_wanted = true
	var water_near_land := shore_of(win, codes, TerrainGrid.WATER, origin, size) if by_water_wanted else {}
	# How near the water each land tile is, once for every kind (0 for the
	# tiles no decoration grows on).
	var land_near := PackedFloat32Array()
	land_near.resize(size * size)
	var any_land_near := false
	if by_land_wanted:
		var land_near_water := nearness_of(win, table([TerrainGrid.WATER]), 2, size)
		var bare := table([TerrainGrid.WATER, TerrainGrid.FORD, TerrainGrid.BROOK, TerrainGrid.ROCK])
		for y in size:
			for x in size:
				if bare[codes[y * size + x]]:
					continue
				var near := _max4(corners_of(land_near_water, x, y, size))
				land_near[y * size + x] = near
				any_land_near = any_land_near or near > 0.0
	for rule: Dictionary in kinds:
		var salt: int = rule["salt"]
		var textures: Array = rule["textures"]
		var ratios: PackedFloat32Array = rule["ratios"]
		var chance: float = rule["chance"]
		var size_px: float = rule["size_px"]
		var min_near: float = rule["min_near"]
		var jitter: float = rule["jitter"]
		var tint: Color = rule["tint"]
		var by_water: bool = rule["by_water"]
		if not by_water and not any_land_near:
			continue
		for y in size:
			for x in size:
				var tx := origin.x + x
				var ty := origin.y + y
				var near := 0.0
				if by_water:
					if codes[y * size + x] != TerrainGrid.WATER or not water_near_land.has(Vector2i(tx, ty)):
						continue
					near = _max4(water_near_land[Vector2i(tx, ty)])
					if near < min_near:
						continue
				else:
					near = land_near[y * size + x]
					if near <= 0.0:
						continue
				if ChunkView.tile_hash(tx, ty, salt) > chance * near:
					continue
				var pick := int(ChunkView.tile_hash(tx, ty, salt + 100) * textures.size()) % textures.size()
				var offset := Vector2(ChunkView.tile_hash(tx, ty, salt + 200) - 0.5, ChunkView.tile_hash(tx, ty, salt + 300) - 0.5) * 600.0
				var centre := Iso.point_to_screen(Vector2(tx * 1000 + 500, ty * 1000 + 500) + offset)
				var scale := 1.0 + (ChunkView.tile_hash(tx, ty, salt + 400) * 2.0 - 1.0) * jitter
				var shown := Vector2(size_px, size_px * ratios[pick] * 0.5) * scale
				out.append([textures[pick], Rect2(centre - shown / 2.0, shown), tint])
	return out


## A table of the 256 codes, 1 for those `codes` names.
static func table(codes: Array) -> PackedByteArray:
	var t := PackedByteArray()
	t.resize(256)
	for c: int in codes:
		t[c] = 1
	return t


## A table of every known code but `except` (unknown is never in it).
static func table_but(except: int) -> PackedByteArray:
	var t := PackedByteArray()
	t.resize(256)
	t.fill(1)
	t[TerrainGrid.UNKNOWN] = 0
	t[except] = 0
	return t


## Nearness of each corner of a chunk `size` tiles wide to the tiles
## `near` accepts (a table of codes), read from `win` (the chunk's codes
## and READ tiles around): 1 at a corner touching one, fading to 0
## width + 1 corners away. Corner (cx, cy), the top of tile (cx, cy), is
## at cy * (size + 1) + cx.
static func nearness_of(win: PackedByteArray, near: PackedByteArray, width: int, size: int) -> PackedFloat32Array:
	# Corners of the chunk and a margin around it: corner c is inside when
	# one of the four tiles meeting there is near. Tiles read once each.
	var side := size + 2 * READ
	var lo := -width
	var span := size + 2 * width + 1
	var tiles := span + 1
	var near_tile := PackedByteArray()
	near_tile.resize(tiles * tiles)
	var any := false
	for ty in tiles:
		var row := (READ + lo - 1 + ty) * side + READ + lo - 1
		for tx in tiles:
			if near[win[row + tx]]:
				near_tile[ty * tiles + tx] = 1
				any = true
	var corners := size + 1
	var values := PackedFloat32Array()
	values.resize(corners * corners)
	if not any:
		return values
	# Corner (lo + i, lo + j) touches tiles i - 1 .. i and j - 1 .. j of
	# the margin, that is indices i .. i + 1 and j .. j + 1 here.
	var inside := PackedByteArray()
	inside.resize(span * span)
	for j in span:
		for i in span:
			var a := j * tiles + i
			if near_tile[a] or near_tile[a + 1] or near_tile[a + tiles] or near_tile[a + tiles + 1]:
				inside[j * span + i] = 1
	# The distance to the nearest inside corner, square rings (the larger
	# of the two offsets): first along each row, then across.
	var far := width + 1
	var along := PackedInt32Array()
	along.resize(span * span)
	for j in span:
		for i in span:
			var best := far
			for d in range(-width, width + 1):
				var k := i + d
				if k >= 0 and k < span and inside[j * span + k] and absi(d) < best:
					best = absi(d)
			along[j * span + i] = best
	for cy in corners:
		for cx in corners:
			var best := far
			var i := cx - lo
			for d in range(-width, width + 1):
				var k := cy - lo + d
				if k >= 0 and k < span:
					best = mini(best, maxi(absi(d), along[k * span + i]))
			values[cy * corners + cx] = 1.0 - float(best) / float(far)
	return values


## The four corners' values of tile (x, y) of the chunk, from corner
## values (nearness_of), clockwise from the top like Iso.diamond.
static func corners_of(values: PackedFloat32Array, x: int, y: int, size: int) -> PackedFloat32Array:
	var c := size + 1
	return PackedFloat32Array([values[y * c + x], values[y * c + x + 1], values[(y + 1) * c + x + 1], values[(y + 1) * c + x]])


## The terrain drawn on tile (x, y) of the window, as a code: its own,
## except a ford, water the water meshes draw, and a brook, a ribbon on
## the ground (BrookLines) over the land around it, forest where the
## tiles around are mostly forest, else grass.
static func drawn_code(win: PackedByteArray, x: int, y: int, size: int) -> int:
	var side := size + 2 * READ
	var code := win[y * side + x]
	if code == TerrainGrid.FORD:
		return TerrainGrid.WATER
	if code != TerrainGrid.BROOK:
		return code
	return land_around(win, x, y, false, size)


## The land most of the tiles around tile (x, y) of the window are,
## FOREST or GRASS; with `drawn`, a brook among them counts as the land
## drawn under it. UNKNOWN when none is land.
static func land_around(win: PackedByteArray, x: int, y: int, drawn: bool, size: int) -> int:
	var side := size + 2 * READ
	var forest := 0
	var grass := 0
	for dy in range(-1, 2):
		for dx in range(-1, 2):
			var near := win[(y + dy) * side + x + dx]
			if near == TerrainGrid.BROOK and drawn:
				near = land_around(win, x + dx, y + dy, false, size)
			if near == TerrainGrid.FOREST:
				forest += 1
			elif near == TerrainGrid.GRASS:
				grass += 1
	if forest + grass == 0 and drawn:
		return TerrainGrid.UNKNOWN
	return TerrainGrid.FOREST if forest > grass else TerrainGrid.GRASS


## For the tiles of terrain `code` in the chunk (`codes`, its own tiles),
## how near each corner is to another, known terrain (the bank): tile ->
## four values, only for the tiles where some corner is near.
static func shore_of(win: PackedByteArray, codes: PackedByteArray, code: int, origin: Vector2i, size: int) -> Dictionary:
	var nearness := nearness_of(win, table_but(code), ChunkView.SHORE_WIDTH, size)
	var result: Dictionary = {}
	for y in size:
		for x in size:
			if codes[y * size + x] != code:
				continue
			var near := corners_of(nearness, x, y, size)
			if near[0] > 0.0 or near[1] > 0.0 or near[2] > 0.0 or near[3] > 0.0:
				result[origin + Vector2i(x, y)] = near
	return result


## Where the water runs at `tile` from `currents` (chunk -> the currents
## as received, the chunks around the one built), as WorldView.current_at
## reads them: what a worker reads instead of the world view's own.
static func current_in(tile: Vector2i, currents: Dictionary, chunk_size: int) -> Vector2:
	var key := Iso.chunk_of(tile, chunk_size)
	var c: Array = currents.get(key, [])
	if c.is_empty():
		return Vector2.ZERO
	var local := tile - key * chunk_size
	return WaterMeshes.current_vector(int(c[local.y * chunk_size + local.x]))


static func _max4(values: PackedFloat32Array) -> float:
	var best := 0.0
	for v in values:
		best = maxf(best, v)
	return best
