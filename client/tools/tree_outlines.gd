extends SceneTree
## Writes the tight outline of every tree image (TreeOutline, D39) to
## art/trees/outlines.json, and prints what each covers. Run it again
## whenever a tree image is drawn anew or added (the test
## `tree outlines hold every texel` fails until it is):
##
##   godot --headless --path client --import && godot --headless --path client -s tools/tree_outlines.gd
##
## Never opens a window. The outlines are read from the imported textures,
## as the graphics card holds them (compressed, with their mipmaps).


func _init() -> void:
	var out := {}
	var files := PackedStringArray()
	for file in DirAccess.get_files_at(Trees.DIR):
		if file.ends_with(".png"):
			files.append(file)
	files.sort()
	print("| image | texels | opaque | quad before | outline | vertices |")
	print("|---|---|---|---|---|---|")
	for file in files:
		var texture: Texture2D = load(Trees.DIR + file)
		var image := texture.get_image()
		var w := image.get_width()
		var h := image.get_height()
		var req := TreeOutline.required(image)
		var points := TreeOutline.polygon(req)
		if points.is_empty():
			push_error("%s draws nothing: no outline" % file)
			continue
		var quad := TreeOutline.area(TreeOutline.fallback(w, h))
		var outline := TreeOutline.area(points)
		var opaque := _opaque_share(image)
		var flat := []
		for v: Vector2 in points:
			flat.append([v.x, v.y])
		# The texture is held padded to a multiple of 4 (the compression's
		# blocks) and drawn at its own size: the points are in the padded
		# image's texels, which the UVs 0..1 span.
		out[file] = {
			"texture": [texture.get_width(), texture.get_height()],
			"image": [w, h],
			"points": flat,
			"covered": snappedf(outline / quad, 0.001),
		}
		print("| %s | %d x %d | %.1f %% | 100 %% | %.1f %% | %d |" % [file, w, h, opaque * float(w * h) / quad * 100.0, outline / quad * 100.0, points.size()])
	var path := Trees.OUTLINES
	var f := FileAccess.open(path, FileAccess.WRITE)
	f.store_string(JSON.stringify(out, "\t", true) + "\n")
	f.close()
	print("written %s: %d outlines" % [path, out.size()])
	quit()


## The share of the image's own texels with alpha > 0 (level 0).
static func _opaque_share(image: Image) -> float:
	var img := image.duplicate() as Image
	if img.is_compressed():
		img.decompress()
	if img.get_format() != Image.FORMAT_RGBA8:
		img.convert(Image.FORMAT_RGBA8)
	var data := img.get_data()
	var n := img.get_width() * img.get_height()
	var opaque := 0
	for i in n:
		if data[i * 4 + 3] > 0:
			opaque += 1
	return float(opaque) / float(n)
