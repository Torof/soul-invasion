class_name Tooltip
extends RefCounted
## Words for what is under the mouse: items by their material's name,
## description and mass, bodies by their number and state. Pure text, no
## network: testable headless.


## The lines for one item, given the catalogue entry of its material.
static func describe_item(material: Dictionary) -> String:
	var name := str(material.get("name", "?"))
	var grams := int(material.get("mass_g", 0))
	var mass := "%.1f kg" % (grams / 1000.0) if grams >= 1000 else "%d g" % grams
	var line := "%s · %s" % [name, mass]
	if bool(material.get("edible", false)):
		line += " · edible"
	var description := str(material.get("description", ""))
	return line if description.is_empty() else line + "\n" + description


## The inventory line for an item: its name and mass, marked when in hand.
static func inventory_line(material: Dictionary, in_hand: bool) -> String:
	var grams := int(material.get("mass_g", 0))
	var mass := "%.1f kg" % (grams / 1000.0) if grams >= 1000 else "%d g" % grams
	var line := "%s · %s" % [str(material.get("name", "?")), mass]
	return "[in hand] " + line if in_hand else line


## The load line: carried over capacity, in kilograms.
static func load_line(load_g: int, capacity_g: int) -> String:
	return "%.1f / %.1f kg" % [load_g / 1000.0, capacity_g / 1000.0]


## Words for a refusal: "cannot pick up: too heavy".
static func refusal_words(act: String, reason: String) -> String:
	var acts := {"pick_up": "pick up", "drop": "drop", "hold": "hold", "stow": "stow", "eat": "eat"}
	var reasons := {
		"unknown": "no such thing",
		"out_of_reach": "out of reach",
		"not_carried": "not carried",
		"too_heavy": "too heavy",
		"not_food": "not food",
	}
	return "cannot %s: %s" % [acts.get(act, act), reasons.get(reason, reason)]


## The line for one body.
static func describe_body(id: int, asleep: bool, piloted: bool) -> String:
	var line := "body %d" % id
	if piloted:
		line += " (you)"
	if asleep:
		line += " · asleep"
	return line
