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
	var h := float(height_cm) / 100.0 * Iso.PX_PER_M * scale
	var w := h * float(texture.get_width()) / float(texture.get_height())
	var foot := Iso.point_to_screen(at)
	return Rect2(foot - Vector2(w / 2.0, h), Vector2(w, h))


## Draws the tree `tree` ({"at": Vector2 in millimetres, "species"}) on
## `canvas`, if its kind has an image.
static func draw(canvas: CanvasItem, tree: Dictionary, height_cm: int) -> void:
	var images := textures(str(tree["species"]))
	if images.is_empty():
		return
	var at: Vector2 = tree["at"]
	var texture: Texture2D = images[variant(at, images.size())]
	canvas.draw_texture_rect(texture, rect(at, height_cm, texture), false)
