class_name Protocol
extends RefCounted
## Wire encoding for client <-> server messages.
##
## This is the only place on the client that knows the messages are JSON
## and that a chunk travels as a binary frame (architecture decisions D25,
## D26). It mirrors the Rust `protocol` crate, which is the single source
## of truth. Every JSON message is a dictionary with a snake_case `type`
## field; a chunk frame decodes into packed arrays (`decode_chunk`).

const PROTOCOL_VERSION := 23
## Chunk frames kept after they left the view (FrameCache), as the
## server's mirror of them.
const CHUNK_CACHE_CHUNKS := 1500
## Entries of `hello`'s `held` the server reads at most.
const HELD_CHUNKS_MAX := 2048

## Bytes of a chunk frame's header.
const CHUNK_HEADER := 16
## Flag of a chunk frame: the chunk has water, its currents follow.
const CHUNK_HAS_WATER := 1
## The bit that marks a natural item's id (`sim::items::NATURAL`).
const NATURAL := 1 << 52
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


## Decodes a chunk's binary frame (the layout in the Rust `ChunkFrame`'s
## doc). `materials`: the content ids of `world_state`'s materials, in
## order. Returns an empty dictionary if the bytes are not a whole frame;
## else:
## - `x`, `y`, `version`: the chunk and its version;
## - `tiles`: PackedByteArray, `side × side` TerrainGrid codes;
## - `currents`: PackedInt32Array, one a tile (heading × 256 + speed, 0
##   where no water runs), empty when the chunk has no water;
## - `trees`, `plants`: PackedInt32Array, three ints each: the kind (an
##   index into `world_state`'s `species` or `flora`), then where it stands
##   in millimetres from the grid's origin, x and y;
## - `items`: Array of {id, x, y, material}, in the order of their ids; a
##   stored item says its own grams too (`mass_g`), a natural one weighs
##   its material's piece.
static func decode_chunk(bytes: PackedByteArray, materials: PackedStringArray) -> Dictionary:
	var size := bytes.size()
	if size < CHUNK_HEADER:
		return {}
	var cx := bytes.decode_s32(0)
	var cy := bytes.decode_s32(4)
	var version := bytes.decode_u32(8)
	var flags := bytes.decode_u8(12)
	var side := bytes.decode_u8(13)
	var tiles_n := side * side
	var at := CHUNK_HEADER + tiles_n
	if size < at:
		return {}
	var tiles := bytes.slice(CHUNK_HEADER, at)
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
	var x0 := cx * side * 1000
	var y0 := cy * side * 1000
	var standing: Array[PackedInt32Array] = []
	for _list in 2:
		if at + 2 > size:
			return {}
		var n := bytes.decode_u16(at)
		at += 2
		if at + n * 5 > size:
			return {}
		var out := PackedInt32Array()
		out.resize(n * 3)
		for i in n:
			out[i * 3] = bytes[at + 4]
			out[i * 3 + 1] = x0 + bytes.decode_u16(at)
			out[i * 3 + 2] = y0 + bytes.decode_u16(at + 2)
			at += 5
		standing.append(out)
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
		"trees": standing[0],
		"plants": standing[1],
		"items": items,
	}


## A material's content id by its index, "" past the list.
static func _material(materials: PackedStringArray, index: int) -> String:
	return materials[index] if index < materials.size() else ""
