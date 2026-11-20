class_name Protocol
extends RefCounted
## Wire encoding for client <-> server messages.
##
## This is the only place on the client that knows the messages are JSON
## and that a chunk and the far view travel as binary messages
## (architecture decisions D25, D26). It mirrors the Rust `protocol` crate,
## which is the single source of truth. Every JSON message is a dictionary
## with a snake_case `type` field; a binary message starts with its kind
## (`binary_kind`): a chunk frame decodes into packed arrays
## (`decode_chunk`), the far view into its letters (`decode_far_view`).

const PROTOCOL_VERSION := 30
## Chunk frames kept after they left the view (FrameCache), as the
## server's mirror of them.
const CHUNK_CACHE_CHUNKS := 1500
## Entries of `hello`'s `held` the server reads at most.
const HELD_CHUNKS_MAX := 2048

## The kind of a binary message, its first byte: a chunk frame.
const BINARY_CHUNK := 0
## The kind of a binary message: the far view.
const BINARY_FAR_VIEW := 1
## Bytes of a chunk frame's header; the length of its ground follows.
const CHUNK_HEADER := 16
## Bytes of the far view's header.
const FAR_VIEW_HEADER := 16
## The most cells a far view may hold: the server's is 256 × 256 (the
## Rust `FAR_CELLS`). A message says its own size (two u16, up to
## 65535 × 65535) and its runs are built by `repeat` before anything else
## looks at them: a hostile server, or a man in the middle on `ws://`,
## made 22 bytes into millions of letters (the second security audit of
## 20 November 2026, C2). Anything past this is refused before a run is
## built.
const FAR_VIEW_CELLS_MAX := 256 * 256
## Flag of a chunk frame: the chunk has water, its currents follow.
const CHUNK_HAS_WATER := 1
## Flag of a chunk frame: the chunk has heights, they follow the currents.
const CHUNK_HAS_HEIGHTS := 2
## The bit that marks a natural item's id (`sim::items::NATURAL`).
const NATURAL := 1 << 52
## The kind of a tree or a plant whose palette index is past its chunk's
## palette: past every list of `world_state`, so unknown.
const UNKNOWN_KIND := 0xFFFF
## A chunk coordinate in a natural item's id: 18 bits.
const CHUNK_MASK := (1 << 18) - 1


## Builds the first message sent after connecting. `world` and `chunks`
## (each [x, y, version], FrameCache.held): the chunks a reconnecting
## client still holds of that world, if any.
static func hello(world: String = "", chunks: Array = []) -> Dictionary:
	var message := {"type": "hello", "protocol_version": PROTOCOL_VERSION}
	if not world.is_empty() and not chunks.is_empty():
		message["held"] = {"world": world, "chunks": chunks}
	return message


## Builds the answer to `chunk_same` for a chunk not held at that version.
static func chunk_missing(x: int, y: int) -> Dictionary:
	return {"type": "chunk_missing", "x": x, "y": y}


## Builds a liveness check.
static func ping(nonce: int) -> Dictionary:
	return {"type": "ping", "nonce": nonce}


## Builds the intention to walk the piloted body to a tile.
static func move_to(x: int, y: int) -> Dictionary:
	return {"type": "move_to", "x": x, "y": y}


## Builds the intention to eat an item within reach.
static func eat(item: int) -> Dictionary:
	return {"type": "eat", "item": item}


## Builds the intention to drink from running water on tile (x, y), the
## body's own or next to it (protocol v24).
static func drink(x: int, y: int) -> Dictionary:
	return {"type": "drink", "x": x, "y": y}


## Builds the intention to take an item off the ground within reach.
static func pick_up(item: int) -> Dictionary:
	return {"type": "pick_up", "item": item}


## Builds the intention to lay a carried item on the body's tile.
static func drop(item: int) -> Dictionary:
	return {"type": "drop", "item": item}


## Builds the intention to take a carried item in hand.
static func hold(item: int) -> Dictionary:
	return {"type": "hold", "item": item}


## Builds the intention to put the item in hand away.
static func stow(item: int) -> Dictionary:
	return {"type": "stow", "item": item}


## Builds the intention to lie down and sleep.
static func sleep() -> Dictionary:
	return {"type": "sleep"}


## Builds the intention to wake up.
static func wake() -> Dictionary:
	return {"type": "wake"}


## Development only (the server must run with --dev): builds the request
## to stand the piloted body at once near a tile.
static func dev_teleport(x: int, y: int) -> Dictionary:
	return {"type": "dev_teleport", "x": x, "y": y}


## Encodes a message dictionary to its wire form.
static func encode(message: Dictionary) -> String:
	# sort_keys=false keeps the field order of the Rust types, so the wire
	# form is byte-identical on both sides.
	return JSON.stringify(message, "", false)


## Decodes a wire string into a message dictionary.
## Returns an empty dictionary if the text is not a valid message.
static func decode(text: String) -> Dictionary:
	var parsed: Variant = JSON.parse_string(text)
	if parsed is Dictionary and (parsed as Dictionary).has("type"):
		return parsed
	return {}


## The kind of a binary message (BINARY_CHUNK, BINARY_FAR_VIEW), -1 for
## no bytes at all.
static func binary_kind(bytes: PackedByteArray) -> int:
	return bytes[0] if bytes.size() > 0 else -1


## Decodes a chunk's binary frame (the layout in the Rust `ChunkFrame`'s
## doc, protocol v30: tiles as a palette and runs, kinds through a
## palette). `materials`: the content ids of `world_state`'s materials, in
## order. Returns an empty dictionary if the bytes are not a whole frame;
## else:
## - `x`, `y`, `version`: the chunk and its version;
## - `tiles`: PackedByteArray, `side × side` TerrainGrid codes;
## - `currents`: PackedInt32Array, one a tile (heading × 256 + speed, 0
##   where no water runs), empty when the chunk has no water;
## - `heights`: PackedInt32Array, one a tile in decimetres above the sea,
##   empty in a world with no relief;
## - `trees`, `plants`: PackedInt32Array, three ints each: the kind (an
##   index into `world_state`'s `species` or `flora`), then where it stands
##   in millimetres from the grid's origin, x and y;
## - `fruit`: PackedByteArray, the fruit each tree shows, in the order of
##   `trees` (D48): its level, 0 none to Trees.FRUIT_LEVELS (8) a rich
##   year at its peak;
## - `plant_fruit`: PackedByteArray, the fruit each plant shows, in the
##   order of `plants`, as the trees' (a berry shrub's, D48);
## - `items`: Array of {id, x, y, material}, in the order of their ids; a
##   stored item says its own grams too (`mass_g`), a natural one weighs
##   its material's piece.
## The growth stage each tree and plant carries (the high four bits of its
## fruit byte, 0 for all today) is not read yet.
static func decode_chunk(bytes: PackedByteArray, materials: PackedStringArray) -> Dictionary:
	var size := bytes.size()
	if size < CHUNK_HEADER + 4 or bytes[0] != BINARY_CHUNK:
		return {}
	var flags := bytes[1]
	var side := bytes[2]
	var cx := bytes.decode_s32(4)
	var cy := bytes.decode_s32(8)
	var version := bytes.decode_u32(12)
	var tiles_n := side * side
	var at := CHUNK_HEADER + 4
	var ground_end := at + bytes.decode_u32(CHUNK_HEADER)
	if ground_end > size or at >= size:
		return {}
	var palette_n := bytes[at]
	at += 1
	if at + palette_n > size:
		return {}
	var palette := bytes.slice(at, at + palette_n)
	at += palette_n
	var tiles := PackedByteArray()
	if palette_n == 1:
		tiles.resize(tiles_n)
		tiles.fill(palette[0])
	elif palette_n > 1:
		var told := 0
		while told < tiles_n:
			if at + 1 >= size or bytes[at] >= palette_n:
				return {}
			var code := palette[bytes[at]]
			var length := bytes[at + 1]
			at += 2
			if length >= 0x80:
				var run := _varint(bytes, at - 1)
				if run.y < 0:
					return {}
				length = run.x
				at = run.y
			if length == 0 or length > tiles_n - told:
				return {}
			tiles.append_array(_filled(code, tiles_n).slice(0, length))
			told += length
	elif tiles_n > 0:
		return {}
	var currents := PackedInt32Array()
	if flags & CHUNK_HAS_WATER:
		currents.resize(tiles_n)
		for i in tiles_n:
			var code := tiles[i]
			if code == TerrainGrid.WATER or code == TerrainGrid.FORD or code == TerrainGrid.BROOK:
				if at + 2 > size:
					return {}
				currents[i] = bytes.decode_u16(at)
				at += 2
	var heights := PackedInt32Array()
	if flags & CHUNK_HAS_HEIGHTS:
		if at + tiles_n * 2 > size:
			return {}
		heights.resize(tiles_n)
		for i in tiles_n:
			heights[i] = bytes.decode_s16(at)
			at += 2
	if at != ground_end:
		return {}
	var x0 := cx * side * 1000
	var y0 := cy * side * 1000
	# Trees then plants: their kinds' palette, then each one in 6 bytes,
	# its palette index and its growth byte (stage << 4 | fruit) after
	# its place.
	var standing: Array[PackedInt32Array] = []
	var fruits: Array[PackedByteArray] = []
	for _list in 2:
		if at + 2 > size:
			return {}
		var n := bytes.decode_u16(at)
		at += 2
		var out := PackedInt32Array()
		out.resize(n * 3)
		var fruit := PackedByteArray()
		fruit.resize(n)
		if n > 0:
			if at >= size:
				return {}
			var kinds_n := bytes[at]
			at += 1
			if at + kinds_n * 2 + n * 6 > size:
				return {}
			# Every index a byte can say: one past the palette names a
			# kind past every list (UNKNOWN_KIND), unknown as a kind past
			# `world_state`'s lists is.
			var kinds := PackedInt32Array()
			kinds.resize(256)
			kinds.fill(UNKNOWN_KIND)
			for k in kinds_n:
				kinds[k] = bytes.decode_u16(at + k * 2)
			at += kinds_n * 2
			for i in n:
				out[i * 3] = kinds[bytes[at + 4]]
				out[i * 3 + 1] = x0 + bytes.decode_u16(at)
				out[i * 3 + 2] = y0 + bytes.decode_u16(at + 2)
				fruit[i] = bytes[at + 5] & 0x0F
				at += 6
		standing.append(out)
		fruits.append(fruit)
	var items: Array = []
	if at + 2 > size:
		return {}
	var natural_n := bytes.decode_u16(at)
	at += 2
	if at + natural_n * 3 > size:
		return {}
	var natural: Array = []
	for i in natural_n:
		var slot := bytes.decode_u16(at)
		var tile := slot >> 6
		natural.append({
			"id": NATURAL | (cx & CHUNK_MASK) << 34 | (cy & CHUNK_MASK) << 16 | slot,
			"x": cx * side + tile % side,
			"y": cy * side + tile / side,
			"material": _material(materials, bytes[at + 2]),
		})
		at += 3
	if at + 2 > size:
		return {}
	var stored_n := bytes.decode_u16(at)
	at += 2
	if at + stored_n * 16 != size:
		return {}
	for i in stored_n:
		items.append({
			"id": bytes.decode_s64(at),
			"x": cx * side + bytes[at + 8],
			"y": cy * side + bytes[at + 9],
			"material": _material(materials, bytes.decode_u16(at + 10)),
			"mass_g": bytes.decode_u32(at + 12),
		})
		at += 16
	# In the order of their ids, as the server lists them: the stored ones
	# first (smaller ids) but for a natural item men laid again.
	items.append_array(natural)
	if stored_n > 0:
		items.sort_custom(func(a: Dictionary, b: Dictionary) -> bool: return int(a["id"]) < int(b["id"]))
	return {
		"x": cx,
		"y": cy,
		"version": version,
		"tiles": tiles,
		"currents": currents,
		"heights": heights,
		"trees": standing[0],
		"fruit": fruits[0],
		"plants": standing[1],
		"plant_fruit": fruits[1],
		"items": items,
	}


## The levels `chunk_fruit` carries (protocol v29, D48): one digit a tree
## or a plant, 0 none to Trees.FRUIT_LEVELS. `ok` false: a character is
## not a level.
static func fruit_levels(digits: String) -> Dictionary:
	var levels := digits.to_ascii_buffer()
	if levels.size() != digits.length():
		return {"ok": false, "levels": PackedByteArray()}
	for i in levels.size():
		var level := levels[i] - 48
		if level < 0 or level > Trees.FRUIT_LEVELS:
			return {"ok": false, "levels": PackedByteArray()}
		levels[i] = level
	return {"ok": true, "levels": levels}


## A chunk frame with the fruit of its trees and plants put in (D48,
## `chunk_fruit`): the same bytes but the fruit of each (the low four
## bits of its growth byte, its stage kept). Empty if the frame does not
## hold as many trees and plants (or is cut short).
static func with_fruit(bytes: PackedByteArray, trees: PackedByteArray, plants: PackedByteArray) -> PackedByteArray:
	var size := bytes.size()
	if size < CHUNK_HEADER + 4 or bytes[0] != BINARY_CHUNK:
		return PackedByteArray()
	var at := CHUNK_HEADER + 4 + bytes.decode_u32(CHUNK_HEADER)
	var out := bytes.duplicate()
	for fruit: PackedByteArray in [trees, plants]:
		if at + 2 > size:
			return PackedByteArray()
		var n := bytes.decode_u16(at)
		at += 2
		if n != fruit.size():
			return PackedByteArray()
		if n == 0:
			continue
		if at >= size:
			return PackedByteArray()
		at += 1 + bytes[at] * 2
		if at + n * 6 > size:
			return PackedByteArray()
		for i in n:
			var growth := at + i * 6 + 5
			out[growth] = (bytes[growth] & 0xF0) | (fruit[i] & 0x0F)
		at += n * 6
	return out


## Decodes the far view's binary message (the layout in the Rust
## `FarView`'s doc): {cell, x, y, width, height, cells}, `cells` one letter
## a cell row by row (`g` grass, `f` forest, `w` river, `o` ford, `b`
## brook, `r` rock, `.` past the map), as the JSON far view of protocol
## v29 carried them. Empty if the bytes are not a whole far view.
static func decode_far_view(bytes: PackedByteArray) -> Dictionary:
	var size := bytes.size()
	if size < FAR_VIEW_HEADER or bytes[0] != BINARY_FAR_VIEW:
		return {}
	var width := bytes.decode_u16(12)
	var height := bytes.decode_u16(14)
	var total := width * height
	if total > FAR_VIEW_CELLS_MAX:
		return {}
	var pieces := PackedStringArray()
	var told := 0
	var at := FAR_VIEW_HEADER
	while told < total:
		if at >= size:
			return {}
		if at + 1 >= size:
			return {}
		var letter := bytes[at]
		var length := bytes[at + 1]
		at += 2
		if length >= 0x80:
			var run := _varint(bytes, at - 1)
			if run.y < 0:
				return {}
			length = run.x
			at = run.y
		if length == 0 or length > total - told:
			return {}
		pieces.append(_letters[letter].repeat(length))
		told += length
	if at != size:
		return {}
	return {
		"cell": bytes.decode_u16(2),
		"x": bytes.decode_s32(4),
		"y": bytes.decode_s32(8),
		"width": width,
		"height": height,
		"cells": "".join(pieces),
	}


## A varint (LEB128, seven bits a byte, the low ones first) at `at`: its
## value and where the next field starts; y -1 when cut short or past 32
## bits.
static func _varint(bytes: PackedByteArray, at: int) -> Vector2i:
	var n := 0
	var shift := 0
	var size := bytes.size()
	while at < size and shift <= 28:
		var byte := bytes[at]
		at += 1
		n |= (byte & 0x7F) << shift
		if byte & 0x80 == 0:
			return Vector2i(n, at) if n <= 0x7FFFFFFF else Vector2i(0, -1)
		shift += 7
	return Vector2i(0, -1)


## Tiles of one terrain by its code, filled once and sliced: a run of the
## tiles.
static var _fills: Array[PackedByteArray] = []
## Each byte as a one-letter string: a run of the far view.
static var _letters: PackedStringArray = _all_letters()


## At least `n` bytes of `code`.
static func _filled(code: int, n: int) -> PackedByteArray:
	if _fills.size() <= code:
		_fills.resize(code + 1)
	if _fills[code].size() < n:
		var fill := PackedByteArray()
		fill.resize(maxi(n, 1024))
		fill.fill(code)
		_fills[code] = fill
	return _fills[code]


## The 256 one-letter strings, by byte.
static func _all_letters() -> PackedStringArray:
	var letters := PackedStringArray()
	for b in 256:
		letters.append(String.chr(b))
	return letters


## A material's content id by its index, "" past the list.
static func _material(materials: PackedStringArray, index: int) -> String:
	return materials[index] if index < materials.size() else ""
