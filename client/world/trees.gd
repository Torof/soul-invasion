class_name Trees
extends RefCounted
## The trees standing on the ground (D37): the server says where each
## stands and of what kind; this draws it, an image of its kind standing
## on the point where its trunk meets the ground. The images are 256 px
## per metre (four times the screen, sharp up to zoom 4), their bottom
## edge at the trunk's foot, centred on it.

const DIR := "res://art/trees/"
## How much a tree's size varies from one to the next, either way.
const SIZE_JITTER := 0.12

## species id -> Array[Texture2D], its variants.
static var _textures: Dictionary = {}
## A soft round shade, dark in the middle, for the shadows on the ground.
static var _shade: Texture2D
## Opacity of the shade right under the trunk, and of the crown's.
const FOOT_SHADE := 0.45
const CROWN_SHADE := 0.18


static func shade() -> Texture2D:
	if _shade == null:
		var g := Gradient.new()
		g.set_color(0, Color(0, 0, 0, 1))
		g.set_color(1, Color(0, 0, 0, 0))
		g.add_point(0.55, Color(0, 0, 0, 0.45))
		var t := GradientTexture2D.new()
		t.gradient = g
		t.fill = GradientTexture2D.FILL_RADIAL
		t.fill_from = Vector2(0.5, 0.5)
		t.fill_to = Vector2(1.0, 0.5)
		t.width = 128
		t.height = 128
		_shade = t
	return _shade


## Draws on `canvas` the shadows of `tree` on the ground: a wide faint
## one under its crown and a darker one hugging its foot, so it stands in
## the grass rather than on it. Drawn before trees and bodies.
static func draw_shadow(canvas: CanvasItem, tree: Dictionary, height_cm: int, origin := Vector2.ZERO) -> void:
	var images := textures(str(tree["species"]))
	if images.is_empty():
		return
	var at: Vector2 = tree["at"]
	var texture: Texture2D = images[variant(at, images.size())]
	var r := rect(at, height_cm, texture)
	var foot := Iso.point_to_screen(at) - origin
	var crown := Vector2(r.size.x * 0.85, r.size.x * 0.42)
	canvas.draw_texture_rect(shade(), Rect2(foot - crown / 2.0 + Vector2(r.size.x * 0.06, 0), crown), false, Color(1, 1, 1, CROWN_SHADE))
	var base := Vector2(Iso.PX_PER_M * 0.9, Iso.PX_PER_M * 0.45) * (r.size.y / (4.0 * Iso.PX_PER_M)) ** 0.5
	canvas.draw_texture_rect(shade(), Rect2(foot - base / 2.0, base), false, Color(1, 1, 1, FOOT_SHADE))


## How much sharper than the hardware's pick the trees' images are read
## (trees.gdshader's mip_bias) at `zoom`: a level sharper up close (zoom 1
## and nearer), easing back to the hardware's own pick at zoom 0.5 and
## farther, where the sharper level costs four times the reading and the
## eye cannot see it.
static func sharpness(zoom: float) -> float:
	return -clampf((zoom - 0.5) / 0.5, 0.0, 1.0)


## The images of a kind of tree: `<id>_adult_01_summer.png`, `_02`, ...
static func textures(species: String) -> Array:
	if not _textures.has(species):
		var found: Array = []
		var n := 1
		while true:
			var path := DIR + "%s_adult_%02d_summer.png" % [species, n]
			if not ResourceLoader.exists(path):
				break
			found.append(load(path))
			n += 1
		_textures[species] = found
	return _textures[species]


## A number in 0..1 from a tree's place and `salt`, the same every time.
static func place_hash(at: Vector2, salt: int) -> float:
	return float(posmod(hash(Vector3i(int(at.x), int(at.y), salt)), 100003)) / 100003.0


## Which of `count` images a tree at `at` shows.
static func variant(at: Vector2, count: int) -> int:
	return posmod(hash(Vector3i(int(at.x), int(at.y), 7)), count)


## The rectangle a tree at `at` (millimetres) of `height_cm` covers on
## screen, its image `texture`: the bottom edge's middle on the foot.
static func rect(at: Vector2, height_cm: int, texture: Texture2D) -> Rect2:
	var scale := 1.0 + (place_hash(at, 3) * 2.0 - 1.0) * SIZE_JITTER
	# The species' true height at the world's scale (D40).
	var h := Scale.shown(float(height_cm) / 100.0) * Iso.PX_PER_M * scale
	var w := h * float(texture.get_width()) / float(texture.get_height())
	var foot := Iso.point_to_screen(at)
	return Rect2(foot - Vector2(w / 2.0, h), Vector2(w, h))


## Draws the tree `tree` ({"at": Vector2 in millimetres, "species"}) on
## `canvas`, if its kind has an image.
## `flags`: 1 when the tree stands in front of the piloted body, +2 when
## it hides something under the mouse (trees.gdshader fades it there).
## `origin`: where the canvas stands, subtracted from the tree's place.
static func draw(canvas: CanvasItem, tree: Dictionary, height_cm: int, flags: int = 0, origin := Vector2.ZERO) -> void:
	var images := textures(str(tree["species"]))
	if images.is_empty():
		return
	var at: Vector2 = tree["at"]
	var texture: Texture2D = images[variant(at, images.size())]
	var r := rect(at, height_cm, texture)
	# The modulate tells the sway shader (trees.gdshader) it is a tree, how
	# tall, and its phase; the shader draws the image untinted.
	var marker := Color(clampf(r.size.y / 1024.0, 0.0, 1.0), 0.25 + 0.25 * float(flags), place_hash(at, 11), 0.94)
	canvas.draw_texture_rect(texture, Rect2(r.position - origin, r.size), false, marker)


## One tree standing on the ground, drawn once and kept by the renderer;
## its parent sorts it with the bodies by its foot (y-sort). It redraws
## only when its see-through flags change.
class TreeNode:
	extends Node2D
	var tree: Dictionary
	var height_cm := 400
	var flags := 0

	func _init(t: Dictionary, h: int) -> void:
		tree = t
		height_cm = h
		position = Iso.point_to_screen(t["at"])
		use_parent_material = true

	func set_flags(f: int) -> void:
		if f != flags:
			flags = f
			queue_redraw()

	func _draw() -> void:
		DevTiming.tally("tree redraws")
		Trees.draw(self, tree, height_cm, flags, position)


## The shadows of a chunk's trees, drawn once on the ground.
class ShadowLayer:
	extends Node2D
	var trees: Array = []
	var heights: Dictionary = {}

	func _draw() -> void:
		for tree: Dictionary in trees:
			Trees.draw_shadow(self, tree, int(heights.get(tree["species"], 400)))

