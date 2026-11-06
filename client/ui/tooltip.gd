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


## The line for one body.
static func describe_body(id: int, asleep: bool, piloted: bool) -> String:
	var line := "body %d" % id
	if piloted:
		line += " (you)"
	if asleep:
		line += " · asleep"
	return line
