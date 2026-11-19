class_name GroundMasks
extends RefCounted
## The ground as a map of numbers (G1 of the ground's lightening, the
## project lead's validation of 19 November 2026): the weights of each
## land terrain's layers worked out once per tile corner on a worker
## (GroundBuild), from the same noise the shader read pixel by pixel, and
## kept in one RGBA8 image a terrain, a pixel a tile corner, TILES ×
## TILES laid over the map again and again (as GroundIndex). The shader
## reads them filtered: the four corners of a tile interpolated over it,
## one read for four masks instead of four noise reads.
##
## Pixel (x, y) is the top corner (north-west on the map) of tile (x, y):
## R the sibling's weight; G the second's, or, for a terrain without a
## second whose bare spots are partly kept (bare_keep), the share kept;
## B the worn grass's weight, or, for a terrain laid in shifted variants,
## the corner's variant (VARIANT_STEP × its three bits); A the dry tone's
## weight before dry_strength.
##
## A tile whose four corners give each layer all of it or none of it is
## uniform: one image covers it whole, and the chunk draws it with the
## cheap material (ground.gdshader with UNIFORM_TILES): one read of these
## masks, one of the image. The others, the mixed tiles (patch borders,
## the forest's spill), keep the full rules of the shader.

const TILES := GroundIndex.TILES
## The terrains with masks: the land grounds with layers.
const TERRAINS := ["grass", "forest"]
## A variant's byte: its bits (0 to 7) times this.
const VARIANT_STEP := 36
## The side of the noise image (ChunkView.noise_image) in pixels.
const NOISE_SIDE := 256
## The masks' numbers and their defaults; ground.json's params of the
## same names override them. Read on the processor only: the shader gets
## the masks, never these.
const DEFAULTS := {
	"noise_scale": 0.09, "sibling_cover": 0.5,
	"second_scale": 0.03, "second_cover": 0.35, "second_softness": 0.1,
	"worn_scale": 0.05, "worn_cover": 0.15, "worn_softness": 0.1,
	"dry_scale": 0.018, "dry_cover": 0.5, "dry_softness": 0.3,
	"variant_scale": 0.05, "bare_keep": 1.0, "has_variants": false,
}
## The layer a uniform tile shows whole.
enum Layer { BASE, SIBLING, SECOND, WORN }

static var _images: Dictionary = {}
static var _textures: Dictionary = {}
static var _upload_due := false


static func texture(terrain: String) -> ImageTexture:
	if not _textures.has(terrain):
		var image := Image.create_empty(TILES, TILES, false, Image.FORMAT_RGBA8)
		_images[terrain] = image
		_textures[terrain] = ImageTexture.create_from_image(image)
	return _textures[terrain]


## What GroundBuild reads to work out a terrain's masks: which layers it
## has (from its ground spec) and its numbers.
static func spec_of(terrain: String) -> Dictionary:
	var spec := DEFAULTS.duplicate()
	var params := ChunkView.ground_params(terrain)
	for name: String in DEFAULTS:
		if params.has(name):
			spec[name] = params[name]
	var layers := ChunkView.ground_layers(terrain)
	for layer: String in ["sibling", "second", "worn"]:
		spec["has_" + layer] = str(layers.get(layer, "")) != ""
	return spec


## The masks of a chunk `size` tiles wide at `origin`, for one terrain
## (`spec`, spec_of): (size + 1)² corners × RGBA, from `noise`
## (ChunkView.noise_image's bytes, NOISE_SIDE²) read as the GPU reads it
## (bilinear, repeating). `planes` keeps the noise read at one scale and
## shift for the other terrains of the chunk.
static func masks_of(origin: Vector2i, size: int, spec: Dictionary, noise: PackedByteArray, planes: Dictionary = {}) -> PackedByteArray:
	var c := size + 1
	var out := PackedByteArray()
	out.resize(c * c * 4)
	var has_sibling := bool(spec["has_sibling"])
	var sib := _plane(origin, c, float(spec["noise_scale"]), Vector2.ZERO, noise, planes) if has_sibling else PackedFloat32Array()
	var sib_lo := 1.0 - float(spec["sibling_cover"]) - 0.15
	var dry := _plane(origin, c, float(spec["dry_scale"]), Vector2(0.8, 0.1), noise, planes)
	var dry_lo := 1.0 - float(spec["dry_cover"]) - float(spec["dry_softness"])
	var dry_hi := 1.0 - float(spec["dry_cover"]) + float(spec["dry_softness"])
	# G: the second, or the bare spots kept.
	var g := PackedFloat32Array()
	var g_lo := 0.0
	var g_hi := 1.0
	if bool(spec["has_second"]):
		g = _plane(origin, c, float(spec["second_scale"]), Vector2(0.2, 0.7), noise, planes)
		g_lo = 1.0 - float(spec["second_cover"]) - float(spec["second_softness"])
		g_hi = 1.0 - float(spec["second_cover"]) + float(spec["second_softness"])
	elif float(spec["bare_keep"]) < 1.0:
		g = _plane(origin, c, 0.06, Vector2(0.9, 0.4), noise, planes)
		g_lo = 1.0 - float(spec["bare_keep"]) - 0.1
		g_hi = 1.0 - float(spec["bare_keep"]) + 0.1
	# B: the variants, or the worn grass.
	var variants := bool(spec["has_variants"])
	var b := PackedFloat32Array()
	var v2 := PackedFloat32Array()
	var v3 := PackedFloat32Array()
	var b_lo := 0.0
	var b_hi := 1.0
	if variants:
		var vs := float(spec["variant_scale"])
		b = _plane(origin, c, vs, Vector2.ZERO, noise, planes)
		v2 = _plane(origin, c, vs * 0.7, Vector2(0.41, 0.17), noise, planes)
		v3 = _plane(origin, c, vs * 1.3, Vector2(0.89, 0.61), noise, planes)
	elif bool(spec["has_worn"]):
		b = _plane(origin, c, float(spec["worn_scale"]), Vector2(0.5, 0.25), noise, planes)
		b_lo = 1.0 - float(spec["worn_cover"]) - float(spec["worn_softness"])
		b_hi = 1.0 - float(spec["worn_cover"]) + float(spec["worn_softness"])
	var has_g := not g.is_empty()
	var has_worn := not variants and not b.is_empty()
	for i in c * c:
		var o := 4 * i
		if has_sibling:
			out[o] = roundi(smoothstep(sib_lo, sib_lo + 0.3, sib[i]) * 255.0)
		if has_g:
			out[o + 1] = roundi(smoothstep(g_lo, g_hi, g[i]) * 255.0)
		if variants:
			# The shader's smoothstep(0.47, 0.53, n) is past its middle
			# exactly when n is past 0.5. The second weight chooses
			# between the first pair and the second, so the third counts
			# only with it, the first only without it: the other is kept
			# at 0, and two corners showing the same variant have the same
			# byte.
			var right := v2[i] >= 0.5
			out[o + 2] = (int(not right and b[i] >= 0.5) | int(right) << 1 | int(right and v3[i] >= 0.5) << 2) * VARIANT_STEP
		elif has_worn:
			out[o + 2] = roundi(smoothstep(b_lo, b_hi, b[i]) * 255.0)
		out[o + 3] = roundi(smoothstep(dry_lo, dry_hi, dry[i]) * 255.0)
	return out


## The noise at every corner of the chunk, read at `scale` and `shift`
## of the texture coordinates (a unit a texture, four metres): the
## shader's texture(noise_texture, uv * scale + shift).r, bilinear and
## repeating, texel centres at half pixels.
static func _plane(origin: Vector2i, c: int, scale: float, shift: Vector2, noise: PackedByteArray, planes: Dictionary) -> PackedFloat32Array:
	var key := Vector3(scale, shift.x, shift.y)
	if planes.has(key):
		return planes[key]
	var n := NOISE_SIDE
	var x0 := PackedInt32Array()
	var x1 := PackedInt32Array()
	var fx := PackedFloat32Array()
	var y0 := PackedInt32Array()
	var y1 := PackedInt32Array()
	var fy := PackedFloat32Array()
	for i in c:
		var sx := (float(origin.x + i) / ChunkView.GROUND_METRES * scale + shift.x) * n - 0.5
		var ix := floori(sx)
		x0.append(posmod(ix, n))
		x1.append(posmod(ix + 1, n))
		fx.append(sx - ix)
		var sy := (float(origin.y + i) / ChunkView.GROUND_METRES * scale + shift.y) * n - 0.5
		var iy := floori(sy)
		y0.append(posmod(iy, n) * n)
		y1.append(posmod(iy + 1, n) * n)
		fy.append(sy - iy)
	var out := PackedFloat32Array()
	out.resize(c * c)
	for j in c:
		var r0 := y0[j]
		var r1 := y1[j]
		var t := fy[j]
		for i in c:
			var a := float(noise[r0 + x0[i]])
			var top := a + (float(noise[r0 + x1[i]]) - a) * fx[i]
			var d := float(noise[r1 + x0[i]])
			var bottom := d + (float(noise[r1 + x1[i]]) - d) * fx[i]
			out[j * c + i] = (top + (bottom - top) * t) / 255.0
	planes[key] = out
	return out


## The layer tile (x, y) of the chunk shows whole, from its four corners'
## masks (`masks`, masks_of), or -1 when the tile is mixed: each layer
## the terrain has all or nothing at the four corners, the base's bare
## spots all kept or all taken by the sibling, one variant.
static func uniform_layer(masks: PackedByteArray, size: int, x: int, y: int, spec: Dictionary) -> int:
	var c := size + 1
	var at := PackedInt32Array([4 * (y * c + x), 4 * (y * c + x + 1), 4 * ((y + 1) * c + x), 4 * ((y + 1) * c + x + 1)])
	var layer: int = Layer.BASE
	var has_sibling := bool(spec["has_sibling"])
	if has_sibling:
		var sib := _whole(masks, at, 0)
		if sib < 0:
			return -1
		if sib == 1:
			layer = Layer.SIBLING
	var second := bool(spec["has_second"])
	if second:
		var g := _whole(masks, at, 1)
		if g < 0:
			return -1
		if g == 1:
			layer = Layer.SECOND
	elif float(spec["bare_keep"]) < 1.0 and layer == Layer.BASE and _whole(masks, at, 1) < 0:
		# The base's bare spots kept or taken by the sibling, all of them
		# or none (the cheap shader does either); not a mixture.
		return -1
	if bool(spec["has_variants"]):
		var v := masks[at[0] + 2]
		if masks[at[1] + 2] != v or masks[at[2] + 2] != v or masks[at[3] + 2] != v:
			return -1
	elif bool(spec["has_worn"]):
		var w := _whole(masks, at, 2)
		if w < 0:
			return -1
		if w == 1:
			layer = Layer.WORN
	return layer


## 1 when channel `ch` is full at the four corners `at`, 0 when empty,
## -1 otherwise.
static func _whole(masks: PackedByteArray, at: PackedInt32Array, ch: int) -> int:
	var v := masks[at[0] + ch]
	if (v != 0 and v != 255) or masks[at[1] + ch] != v or masks[at[2] + ch] != v or masks[at[3] + ch] != v:
		return -1
	return 1 if v == 255 else 0


## Lays a chunk's masks (terrain -> masks_of's bytes) at `lo`, the far
## edges' corners landing on the next chunk's first, which compute the
## same values (a corner's masks depend on its place alone).
static func write(lo: Vector2i, size: int, masks: Dictionary) -> void:
	var c := size + 1
	for terrain: String in masks:
		texture(terrain)
		var image: Image = _images[terrain]
		var chunk := Image.create_from_data(c, c, false, Image.FORMAT_RGBA8, masks[terrain])
		var x0 := posmod(lo.x, TILES)
		var y0 := posmod(lo.y, TILES)
		# Cut where the map's repetition wraps.
		var w0 := mini(c, TILES - x0)
		var h0 := mini(c, TILES - y0)
		image.blit_rect(chunk, Rect2i(0, 0, w0, h0), Vector2i(x0, y0))
		if w0 < c:
			image.blit_rect(chunk, Rect2i(w0, 0, c - w0, h0), Vector2i(0, y0))
		if h0 < c:
			image.blit_rect(chunk, Rect2i(0, h0, w0, c - h0), Vector2i(x0, 0))
		if w0 < c and h0 < c:
			image.blit_rect(chunk, Rect2i(w0, h0, c - w0, c - h0), Vector2i.ZERO)
	if not _upload_due and not masks.is_empty():
		_upload_due = true
		_upload.call_deferred()


static func _upload() -> void:
	_upload_due = false
	for terrain: String in _textures:
		(_textures[terrain] as ImageTexture).update(_images[terrain])


## A corner's four bytes, for the tests.
static func at(terrain: String, corner: Vector2i) -> PackedByteArray:
	texture(terrain)
	var c := (_images[terrain] as Image).get_pixel(posmod(corner.x, TILES), posmod(corner.y, TILES))
	return PackedByteArray([c.r8, c.g8, c.b8, c.a8])
