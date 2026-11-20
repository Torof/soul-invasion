class_name GroundPages
extends Node2D
## The land's ground (the grass's and the forest's canvases, ChunkView.
## GroundCanvas, and the far view under them, FarView) baked into screen
## pages at the current zoom (D39): they draw on a canvas of their own,
## which a grid of PageLedger.SIZE pixel viewports sees, each shifted by a
## whole page; the world view shows the pages' pictures where they were
## (z -7), copied as they are, under the water, the brooks, the fog's veil,
## the plants, the shadows and the trees, which stay live. A page is opaque
## and holds 10 bits a channel, as the screen does: the same pixels, never
## rounded twice. The camera is laid on whole screen pixels and the zoom moves by
## steps, so the land of a zoom is one fixed picture that only slides: a
## page is rendered when it comes into the ring around the screen, when the
## ground under it is laid again (a chunk, its slopes) and, all of them, on
## a zoom step, a teleport or a change of the ground's look (a material's
## dial, the night). Every other frame reads the pages and shades nothing.
##
## Development: SOUL_INVASION_GROUND_PAGES=0 draws the canvases live, as
## before (set_enabled).

## Whether the land is drawn through the pages.
static var enabled := OS.get_environment("SOUL_INVASION_GROUND_PAGES") != "0"
## The canvas the land's canvases draw on, and the item they hang from
## (it carries the world view's modulate: the night).
static var _world: World2D
static var _root := RID()
## Where the ground changed since the pages last looked, in world pixels.
static var _changed: Array[Rect2] = []
## Bumped when the land's look changes as a whole (a material's dial).
static var look := 0
## Pages rendered since the start (the tests, the development counters).
static var rendered := 0

var _ledger := PageLedger.new()
## page -> its viewport; the viewports free.
var _viewports: Dictionary = {}
var _free: Array[SubViewport] = []
var _all: Array[SubViewport] = []
## The pages' pictures, drawn in the world view.
var _shown := RID()
var _shown_dirty := false
var _copy: ShaderMaterial
var _look_seen := -1
var _modulate_seen := Color(-1, -1, -1, -1)
var _source: CanvasItem


## The item the land's canvases hang from, made on first use.
static func root() -> RID:
	if not _root.is_valid():
		_world = World2D.new()
		_root = RenderingServer.canvas_item_create()
		RenderingServer.canvas_item_set_parent(_root, _world.canvas)
	return _root


## The ground under `rect` (world pixels) changed.
static func changed(rect: Rect2) -> void:
	if enabled and rect.has_area():
		_changed.append(rect)


## The ground of tiles `lo` to `lo + size` changed in the shader's
## indexes (GroundIndex, HeightIndex): what they draw, grown by a tile for
## what the shader reads around a tile (the forest's spill, the slopes'
## filtering).
static func changed_tiles(lo: Vector2i, size: int) -> void:
	if not enabled:
		return
	var a := lo - Vector2i.ONE
	var b := lo + Vector2i(size, size) + Vector2i.ONE
	var top := Iso.tile_to_screen(a.x, a.y)
	var right := Iso.tile_to_screen(b.x, a.y)
	var bottom := Iso.tile_to_screen(b.x, b.y)
	var left := Iso.tile_to_screen(a.x, b.y)
	var r := Rect2(top, Vector2.ZERO).expand(right).expand(bottom).expand(left)
	changed(r.grow(Iso.TILE_H))


## The land's look changed as a whole: every page again.
static func changed_look() -> void:
	look += 1


## Development: the land drawn through the pages or live.
static func set_enabled(on: bool, tree: SceneTree) -> void:
	if on == enabled:
		return
	enabled = on
	_changed.clear()
	look += 1
	for canvas in tree.get_nodes_in_group("ground_canvases"):
		(canvas as CanvasItem).queue_redraw()


## `source`: the node whose modulate the land takes (the world view).
func _init(source: CanvasItem = null) -> void:
	_source = source


func _ready() -> void:
	# On the world's canvas itself, at the land's place: the pages hold the
	# far view (-20) and the land (-7, -6), nothing lies between them on
	# the screen, and over them come the water (-5) and all the rest.
	top_level = true
	z_as_relative = false
	z_index = -7
	# Copied as they are: a page holds the screen's pixels under the
	# water, opaque, at the screen's own depth (10 bits a channel), so
	# nothing is blended or rounded twice.
	_copy = ShaderMaterial.new()
	_copy.shader = Shader.new()
	_copy.shader.code = "shader_type canvas_item;\nrender_mode blend_disabled, unshaded;\n"
	_shown = RenderingServer.canvas_item_create()
	RenderingServer.canvas_item_set_parent(_shown, get_canvas_item())
	RenderingServer.canvas_item_set_material(_shown, _copy.get_rid())
	RenderingServer.canvas_item_set_default_texture_filter(_shown, RenderingServer.CANVAS_ITEM_TEXTURE_FILTER_NEAREST)
	root()
	RenderingServer.frame_pre_draw.connect(_lay_pages)


func _exit_tree() -> void:
	if RenderingServer.frame_pre_draw.is_connected(_lay_pages):
		RenderingServer.frame_pre_draw.disconnect(_lay_pages)


func _notification(what: int) -> void:
	if what == NOTIFICATION_PREDELETE:
		if _shown.is_valid():
			RenderingServer.free_rid(_shown)
		# The land's canvas goes with the world view.
		if _root.is_valid():
			RenderingServer.free_rid(_root)
			_root = RID()


## The screen's transform from the world's pixels to the screen's, the one
## the frame is drawn with.
func _screen_transform() -> Transform2D:
	return get_viewport().get_final_transform() * get_viewport().get_canvas_transform()


## Keeps enough viewports for the screen (made here, never while the frame
## is laid out).
func _process(_delta: float) -> void:
	if not enabled:
		if not _all.is_empty():
			_release_all()
		return
	var want := PageLedger.most_held(_screen_size())
	while _all.size() < want:
		var page := SubViewport.new()
		page.size = Vector2i(PageLedger.SIZE, PageLedger.SIZE)
		page.transparent_bg = false
		page.disable_3d = true
		page.gui_disable_input = true
		page.render_target_update_mode = SubViewport.UPDATE_DISABLED
		page.world_2d = _world
		add_child(page)
		_all.append(page)
		_free.append(page)


func _screen_size() -> Vector2:
	return get_viewport().get_final_transform().basis_xform(get_viewport().get_visible_rect().size).abs()


## Just before the frame is drawn, the camera in its place for good: the
## pages the screen and its ring need, those to render, and the pictures.
func _lay_pages() -> void:
	if not enabled or not is_inside_tree() or _all.is_empty():
		return
	var t := _screen_transform()
	var scale := t.x.x
	# The world pixel v lies at t.x.x * v + origin on the screen; the
	# pages hold it at scale * v + (origin - whole) - SIZE × page, so the
	# screen's pixel X is the page's X - whole - SIZE × page, on whole
	# pixels, the arithmetic of the live drawing kept (the same products,
	# the same sums).
	var whole := t.origin.round()
	var screen := Rect2(-whole, _screen_size())
	for key in _ledger.see(screen, scale):
		_release(key)
	var modulate_now := _source.modulate if _source != null else Color.WHITE
	if modulate_now != _modulate_seen:
		_modulate_seen = modulate_now
		RenderingServer.canvas_item_set_modulate(_root, modulate_now)
		_ledger.dirty_all()
	if _look_seen != look:
		_look_seen = look
		_ledger.dirty_all()
		_changed.clear()
	for rect in _changed:
		_ledger.dirty(Rect2(t * rect.position - whole, rect.size * scale).grow(2.0).abs())
	_changed.clear()
	for key in _ledger.due():
		var page: SubViewport = _viewports.get(key)
		if page == null:
			page = _free.pop_back()
			_viewports[key] = page
			_shown_dirty = true
		page.canvas_transform = Transform2D(t.x, t.y, t.origin - whole - Vector2(key * PageLedger.SIZE))
		page.render_target_update_mode = SubViewport.UPDATE_ONCE
		rendered += 1
	if _shown_dirty:
		_shown_dirty = false
		_draw_pages(t, whole)


## The pages' pictures where they lie on the screen, in world pixels.
func _draw_pages(t: Transform2D, whole: Vector2) -> void:
	RenderingServer.canvas_item_clear(_shown)
	var back := t.affine_inverse()
	for key: Vector2i in _viewports:
		var page: SubViewport = _viewports[key]
		var at := Vector2(key * PageLedger.SIZE) + whole
		var a := back * at
		var b := back * (at + Vector2(PageLedger.SIZE, PageLedger.SIZE))
		RenderingServer.canvas_item_add_texture_rect(_shown, Rect2(a, b - a), page.get_texture().get_rid())


func _release(key: Vector2i) -> void:
	var page: SubViewport = _viewports.get(key)
	if page == null:
		return
	_viewports.erase(key)
	page.render_target_update_mode = SubViewport.UPDATE_DISABLED
	_free.append(page)
	_shown_dirty = true


func _release_all() -> void:
	_viewports.clear()
	_free.clear()
	for page in _all:
		page.queue_free()
	_all.clear()
	_ledger = PageLedger.new()
	RenderingServer.canvas_item_clear(_shown)
	_modulate_seen = Color(-1, -1, -1, -1)


## What the pages hold: their number and their bytes on the card (RGBA8).
func counts() -> Dictionary:
	return {"pages": _all.size(), "page_bytes": _all.size() * PageLedger.SIZE * PageLedger.SIZE * 4, "stale": _ledger.stale(), "rendered": rendered}
