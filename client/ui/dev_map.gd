class_name DevMap
extends CanvasLayer
## Development only, when the server runs with --dev: the whole valley,
## coarse (the server's overview, one letter a cell), turned as the game
## shows it: tile (0, 0) at the top, x down to the right, y down to the
## left. M opens and closes it, Escape closes it; a click stands the body
## there at once (WorldView.teleport_to).

const BACKGROUND := Color(0.07, 0.09, 0.11, 0.92)
const COLOURS := {
	"g": Color(0.50, 0.67, 0.32),
	"f": Color(0.20, 0.38, 0.18),
	"w": Color(0.12, 0.42, 0.66),
	"o": Color(0.55, 0.78, 0.82),
	"b": Color(0.27, 0.59, 0.78),
	"r": Color(0.41, 0.39, 0.37),
}
const MARKER := Color(0.95, 0.25, 0.20)

## The world view (WorldView), to know where the body stands and to
## teleport; kept untyped so this map stands alone in the tests.
var world: Node
var _canvas: Control
var _texture: ImageTexture
var _cell := 8
var _size := Vector2i.ZERO


func _ready() -> void:
	layer = 50
	visible = false
	_canvas = Control.new()
	_canvas.set_anchors_preset(Control.PRESET_FULL_RECT)
	_canvas.mouse_filter = Control.MOUSE_FILTER_STOP
	_canvas.draw.connect(_paint)
	_canvas.gui_input.connect(_on_input)
	add_child(_canvas)


## Takes the server's overview (Protocol: dev_overview; the main scene
## passes it on).
func show_overview(cell: int, width: int, height: int, cells: String) -> void:
	if width <= 0 or height <= 0 or cells.length() < width * height:
		return
	_cell = cell
	_size = Vector2i(width, height)
	var image := Image.create(width, height, false, Image.FORMAT_RGBA8)
	for y in height:
		for x in width:
			image.set_pixel(x, y, COLOURS.get(cells[y * width + x], Color.MAGENTA))
	_texture = ImageTexture.create_from_image(image)


## Whether there is an overview to show.
func available() -> bool:
	return _texture != null


func _unhandled_input(event: InputEvent) -> void:
	if not event is InputEventKey or not event.pressed or event.echo:
		return
	if event.keycode == KEY_M and available():
		visible = not visible
		get_viewport().set_input_as_handled()
	elif event.keycode == KEY_ESCAPE and visible:
		visible = false
		get_viewport().set_input_as_handled()


func _process(_delta: float) -> void:
	if visible:
		_canvas.queue_redraw()


## Pixels a cell's side takes on screen, and where cell (0, 0) is drawn.
func _layout() -> Array:
	var room := _canvas.size - Vector2(80, 140)
	var across := float(_size.x + _size.y)
	var side := minf(room.x / across, 2.0 * room.y / across)
	var top := Vector2(_canvas.size.x / 2.0 + (_size.y - _size.x) * side / 2.0, 90.0)
	return [side, top]


## From cells (x, y) to the screen, turned as the game.
func _to_screen(cell: Vector2, side: float, top: Vector2) -> Vector2:
	return top + Vector2((cell.x - cell.y) * side, (cell.x + cell.y) * side / 2.0)


## From the screen back to cells.
func _to_cell(point: Vector2, side: float, top: Vector2) -> Vector2:
	var p := point - top
	var a := p.x / side
	var b := 2.0 * p.y / side
	return Vector2((a + b) / 2.0, (b - a) / 2.0)


func _paint() -> void:
	_canvas.draw_rect(Rect2(Vector2.ZERO, _canvas.size), BACKGROUND)
	if _texture == null:
		return
	var layout := _layout()
	var side: float = layout[0]
	var top: Vector2 = layout[1]
	_canvas.draw_set_transform_matrix(Transform2D(Vector2(side, side / 2.0), Vector2(-side, side / 2.0), top))
	_canvas.draw_texture(_texture, Vector2.ZERO)
	_canvas.draw_set_transform_matrix(Transform2D.IDENTITY)
	if world != null:
		var tile: Vector2i = world.call("piloted_tile")
		if tile.x >= 0:
			var at := _to_screen(Vector2(tile) / float(_cell), side, top)
			_canvas.draw_circle(at, 7.0, Color.WHITE)
			_canvas.draw_circle(at, 5.0, MARKER)
	var font := ThemeDB.fallback_font
	_canvas.draw_string(font, Vector2(24, 36), "Development map (--dev): click to go there at once. M or Escape closes. Ctrl+click on the ground does the same nearby.", HORIZONTAL_ALIGNMENT_LEFT, -1, 16, Color(0.9, 0.92, 0.94))
	var hover := _to_cell(_canvas.get_local_mouse_position(), side, top) * _cell
	if hover.x >= 0 and hover.y >= 0 and hover.x < _size.x * _cell and hover.y < _size.y * _cell:
		_canvas.draw_string(font, Vector2(24, 60), "tile (%d, %d)" % [int(hover.x), int(hover.y)], HORIZONTAL_ALIGNMENT_LEFT, -1, 14, Color(0.7, 0.75, 0.78))


func _on_input(event: InputEvent) -> void:
	if not (event is InputEventMouseButton and event.pressed and event.button_index == MOUSE_BUTTON_LEFT):
		return
	var layout := _layout()
	var cell := _to_cell(event.position, layout[0], layout[1])
	if cell.x < 0 or cell.y < 0 or cell.x >= _size.x or cell.y >= _size.y:
		return
	if world != null:
		world.call("teleport_to", Vector2i(int(cell.x * _cell), int(cell.y * _cell)))
	visible = false
