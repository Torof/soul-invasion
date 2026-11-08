class_name Protocol
extends RefCounted
## Wire encoding for client <-> server messages.
##
## This is the only place on the client that knows the messages are JSON
## (architecture decision D26). It mirrors the Rust `protocol` crate, which
## is the single source of truth. Every message is a dictionary with a
## snake_case `type` field.

const PROTOCOL_VERSION := 11


## Builds the first message sent after connecting.
static func hello() -> Dictionary:
	return {"type": "hello", "protocol_version": PROTOCOL_VERSION}


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
