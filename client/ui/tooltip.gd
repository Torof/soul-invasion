class_name Tooltip
extends RefCounted
## Words for what is under the mouse: items by their material's name,
## description and mass, bodies by their number and state. Pure text, no
## network: testable headless.


## The lines for one item, given the catalogue entry of its material and
## the item's own grams (D35); without them (-1), its material's piece.
static func describe_item(material: Dictionary, grams := -1) -> String:
	var name := str(material.get("name", "?"))
	var line := "%s · %s" % [name, mass_words(material, grams)]
	if bool(material.get("edible", false)):
		line += " · edible"
	var description := str(material.get("description", ""))
	return line if description.is_empty() else line + "\n" + description


## The inventory line for an item: its name and its own grams (or, -1,
## its material's piece), marked when in hand.
static func inventory_line(material: Dictionary, in_hand: bool, grams := -1) -> String:
	var line := "%s · %s" % [str(material.get("name", "?")), mass_words(material, grams)]
	return "[in hand] " + line if in_hand else line


## A mass in words: grams under a kilogram, else kilograms to a decimal.
## `grams` -1: the material's piece.
static func mass_words(material: Dictionary, grams: int) -> String:
	if grams < 0:
		grams = int(material.get("mass_g", 0))
	return "%.1f kg" % (grams / 1000.0) if grams >= 1000 else "%d g" % grams


## The load line: carried over capacity, in kilograms.
static func load_line(load_g: int, capacity_g: int) -> String:
	return "%.1f / %.1f kg" % [load_g / 1000.0, capacity_g / 1000.0]


## Words for a refusal: "cannot pick up: too heavy".
static func refusal_words(act: String, reason: String) -> String:
	var acts := {"pick_up": "pick up", "drop": "drop", "hold": "hold", "stow": "stow", "eat": "eat", "drink": "drink"}
	var reasons := {
		"unknown": "no such thing",
		"out_of_reach": "out of reach",
		"not_carried": "not carried",
		"too_heavy": "too heavy",
		"not_food": "not food",
		"not_water": "no running water there",
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


## Lines naming the plants on a tile: their kinds' names, each once, with
## how many when there are several ("Bramble ×3"). `kinds` are indices
## into `flora` (Net.flora).
static func describe_plants(kinds: Array, flora: Array) -> PackedStringArray:
	var counts := {}
	var order: Array = []
	for kind: int in kinds:
		if kind < 0 or kind >= flora.size():
			continue
		var name := str((flora[kind] as Dictionary).get("name", "?"))
		if not counts.has(name):
			order.append(name)
		counts[name] = int(counts.get(name, 0)) + 1
	var lines := PackedStringArray()
	for name: String in order:
		lines.append(name if counts[name] == 1 else "%s ×%d" % [name, counts[name]])
	return lines
