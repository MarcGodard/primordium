## Environment levers for any scale: one log slider per lever (x1/4 .. x4 on a real rate in the
## model). The owner passes the levers and two callables: `apply(key, mult) -> bool` (false: not
## applied right now, e.g. mid fast-forward) and `read() -> Dictionary` (key -> current mult).
extends PanelContainer

var levers: Array = []
## Slider reach in doublings each way (2: x1/4 .. x4). Set before `setup`.
var span := 2.0
## Levers side by side (2 for a long list, e.g. a 20-element recipe). Set before `setup`.
var columns := 1
var apply: Callable
var read: Callable
var sliders := {}
var labels := {}

## `defs`: [key, name, help] per lever.
func setup(title: String, defs: Array, apply_fn: Callable, read_fn: Callable, note: String) -> void:
	levers = defs
	apply = apply_fn
	read = read_fn
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(0.03, 0.05, 0.08, 0.9)
	sb.set_corner_radius_all(8)
	sb.content_margin_left = 14
	sb.content_margin_right = 14
	sb.content_margin_top = 10
	sb.content_margin_bottom = 10
	add_theme_stylebox_override("panel", sb)
	custom_minimum_size = Vector2(330, 0)
	var box := VBoxContainer.new()
	add_child(box)
	var head := Label.new()
	head.text = title
	head.add_theme_font_size_override("font_size", 14)
	box.add_child(head)
	var grid := GridContainer.new()
	grid.columns = 2 * columns
	grid.add_theme_constant_override("h_separation", 8)
	box.add_child(grid)
	for lv in defs:
		var l := Label.new()
		l.custom_minimum_size = Vector2(120 if columns == 1 else 64, 0)
		l.add_theme_font_size_override("font_size", 13)
		l.tooltip_text = lv[2]
		l.mouse_filter = Control.MOUSE_FILTER_PASS
		grid.add_child(l)
		var sl := HSlider.new()
		sl.min_value = -span
		sl.max_value = span
		sl.step = 0.25
		sl.custom_minimum_size = Vector2(180 if columns == 1 else 100, 0)
		sl.tooltip_text = lv[2]
		sl.focus_mode = Control.FOCUS_NONE
		# panels sit over the world, where the wheel zooms: it must not move a lever
		sl.scrollable = false
		var key: String = lv[0]
		sl.value_changed.connect(func(v: float) -> void:
			apply.call(key, pow(2.0, v))
			refresh())
		grid.add_child(sl)
		sliders[key] = sl
		labels[key] = l
	var n := Label.new()
	n.text = note
	n.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	n.custom_minimum_size = Vector2(300, 0)
	n.add_theme_font_size_override("font_size", 11)
	n.add_theme_color_override("font_color", Color(0.6, 0.7, 0.8))
	box.add_child(n)
	visible = false
	refresh()

## Labels and sliders from the model's actual values (sliders move without re-firing).
func refresh() -> void:
	var lv: Dictionary = read.call()
	if lv.is_empty():
		return
	for x in levers:
		var m: float = lv.get(x[0], 1.0)
		labels[x[0]].text = "%s  x%s" % [x[1], ("%.2f" % m).trim_suffix("0").trim_suffix("0").trim_suffix(".")]
		sliders[x[0]].set_value_no_signal(log(m) / log(2.0))

func lock(on: bool) -> void:
	for sl in sliders.values():
		sl.editable = not on

## Non-default levers as text, e.g. " (waves x0.25)"; "" when all are x1.
func note() -> String:
	var lv: Dictionary = read.call()
	var parts := []
	for x in levers:
		var m: float = lv.get(x[0], 1.0)
		if abs(m - 1.0) > 0.01:
			parts.append("%s x%.2f" % [x[1].to_lower(), m])
	return "" if parts.is_empty() else " (" + ", ".join(PackedStringArray(parts)) + ")"
