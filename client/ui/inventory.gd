class_name Inventory
extends PanelContainer
## What the piloted body carries: the item in hand first, then the rest,
## with masses and the load. Clicking an item offers what can be done
## with it; the server judges (D35).

@onready var _title: Label = %InventoryTitle
@onready var _list: ItemList = %Items
@onready var _hold_button: Button = %Hold
@onready var _stow_button: Button = %Stow
@onready var _drop_button: Button = %Drop
@onready var _eat_button: Button = %Eat

## material id -> catalogue entry
var _materials: Dictionary = {}
var _held: Dictionary = {}
var _carried: Array = []
var _load_g := 0
var _capacity_g := 0
var _body := -1


func _ready() -> void:
	Net.world_state.connect(_on_world_state)
	Net.connected.connect(func(_tick: int, body: int) -> void: _body = body)
	Net.body_assigned.connect(_on_body_assigned)
	Net.inventory.connect(_on_inventory)
	_list.item_selected.connect(func(_index: int) -> void: _refresh_buttons())
	_hold_button.pressed.connect(func() -> void: _act(Net.send_hold))
	_stow_button.pressed.connect(func() -> void: _act(Net.send_stow))
	_drop_button.pressed.connect(func() -> void: _act(Net.send_drop))
	_eat_button.pressed.connect(func() -> void: _act(Net.send_eat))
	_refresh()


func _on_world_state(_w: int, _h: int, _c: int, materials: Array, _night: bool) -> void:
	_materials.clear()
	for m in materials:
		_materials[str(m.get("id", ""))] = m
	_held = {}
	_carried = []
	_refresh()


func _on_body_assigned(body: int) -> void:
	_body = body
	_held = {}
	_carried = []
	_load_g = 0
	_refresh()


func _on_inventory(body: int, held: Dictionary, carried: Array, load_g: int, capacity_g: int) -> void:
	if body != _body:
		return
	_held = held
	_carried = carried
	_load_g = load_g
	_capacity_g = capacity_g
	_refresh()


func _selected() -> Dictionary:
	var selected := _list.get_selected_items()
	if selected.is_empty():
		return {}
	var index: int = selected[0]
	if not _held.is_empty():
		if index == 0:
			return _held
		index -= 1
	return _carried[index] if index < _carried.size() else {}


func _act(send: Callable) -> void:
	var item := _selected()
	if not item.is_empty():
		send.call(int(item.get("id", -1)))


func _refresh() -> void:
	_title.text = "carrying " + Tooltip.load_line(_load_g, _capacity_g) if _body >= 0 else "watching"
	_list.clear()
	if not _held.is_empty():
		_list.add_item(Tooltip.inventory_line(_materials.get(str(_held.get("material", "")), {}), true))
	for item in _carried:
		_list.add_item(Tooltip.inventory_line(_materials.get(str(item.get("material", "")), {}), false))
	_refresh_buttons()


func _refresh_buttons() -> void:
	var item := _selected()
	var none := item.is_empty()
	var in_hand := not none and item == _held
	var edible: bool = (not none) and bool(_materials.get(str(item.get("material", "")), {}).get("edible", false))
	_hold_button.disabled = none or in_hand
	_stow_button.disabled = none or not in_hand
	_drop_button.disabled = none
	_eat_button.disabled = none or not edible
