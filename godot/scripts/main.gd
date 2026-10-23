## Root: the hub, one scale view at a time, the HUD, a scale bar and warp transitions.
## Starts in the hub unless --scale=K is given.
## Capture mode (no interaction, exits): godot --path godot -- --shot=out.png [--scale=K] [--warm=SWEEPS] [--zoom=Z] [--focus=X,Y] [--frames=N]
extends Node

var view: Node
var views: Array = []
var hud: CanvasLayer
var hub: Node3D
var in_hub := true
var shot := ""
var frames := 20
var bar: HBoxContainer
var bar_buttons: Array = []
var warp: ColorRect
var warp_title: Label
var warp_sub: Label
var travelling := false
## Captures normally skip the warp; --warp=K plays it so a frame mid-transition can be checked.
var show_warp := false

func _ready() -> void:
	for path in ["res://scripts/periodic_view.gd", "res://scripts/chem_view.gd", "res://scripts/pool_view.gd", "res://scripts/pond_view.gd", "res://scripts/cell_view.gd", "res://scripts/coast_view.gd", "res://scripts/reef_view.gd"]:
		var v: Node = load(path).new()
		add_child(v)
		views.append(v)
	view = views[0]
	for v in views:
		v.activate(false)
	hud = load("res://scripts/hud.gd").new()
	hud.view = view
	add_child(hud)
	hub = load("res://scripts/hub.gd").new()
	add_child(hub)
	hub.chosen.connect(travel)
	_build_bar()
	_build_warp()
	enter_hub()
	for a in OS.get_cmdline_user_args():
		var kv: PackedStringArray = a.trim_prefix("--").split("=")
		if kv.size() != 2:
			continue
		match kv[0]:
			"shot":
				shot = kv[1]
				# captures must not steal focus or eat the user's keystrokes
				DisplayServer.window_set_flag(DisplayServer.WINDOW_FLAG_NO_FOCUS, true)
				view.set_process_unhandled_input(false)
				hub.set_process_unhandled_input(false)
				hub.hover_enabled = false
			"frames": frames = int(kv[1])
			"scale": switch_to(int(kv[1]))
			"pick": view.pick_longest()
			"explain": hud.explaining = kv[1] == "1"
			"warp":
				show_warp = true
				travel(int(kv[1]))
			"view3d":
				if view.has_method("set_3d"):
					view.set_3d(kv[1] == "1")
			"lineage":
				if view.has_method("set_lineage"):
					view.set_lineage(kv[1] == "1")
			"warm": view.warm(int(kv[1]))
			"zoom":
				if view.cam is Camera2D:
					view.cam.zoom = Vector2(float(kv[1]), float(kv[1]))
				else:
					view.dist = float(kv[1])
			"focus":
				var xy := kv[1].split(",")
				if view.cam is Camera2D:
					view.cam.position = Vector2(float(xy[0]), float(xy[1])) * view.CELL
				else:
					view.target = Vector3(float(xy[0]), view.target.y, -float(xy[1]))

## Show scale k now, no transition.
func switch_to(k: int) -> void:
	view = views[posmod(k, views.size())]
	in_hub = false
	hub.activate(false)
	for v in views:
		v.activate(v == view)
	hud.view = view
	hud.visible = true
	hud.set_process(true)
	hud.set_process_unhandled_input(true)
	bar.visible = true
	for i in bar_buttons.size():
		var b: Button = bar_buttons[i]
		b.add_theme_color_override("font_color", hub.SCALES[i][2] if views[i] == view else Color(0.55, 0.62, 0.72))
		b.add_theme_font_size_override("font_size", 14 if views[i] == view else 12)

func enter_hub() -> void:
	in_hub = true
	for v in views:
		v.activate(false)
	hub.activate(true)
	# the HUD and its guide belong to the scales: off entirely in the hub (its guide sits on
	# its own layer, so hiding the HUD alone would not hide it)
	hud.visible = false
	hud.explaining = false
	hud.explain_panel.visible = false
	hud.set_process(false)
	hud.set_process_unhandled_input(false)
	bar.visible = false

## Travel to scale k (or the hub when k < 0) through a warp: streaks rush outward when
## climbing to a bigger scale, inward when diving smaller or in from the hub.
func travel(k: int) -> void:
	if travelling:
		return
	var from := -1 if in_hub else views.find(view)
	if k == from:
		return
	if shot != "" and not show_warp:
		if k < 0:
			enter_hub()
		else:
			switch_to(k)
		return
	travelling = true
	var outward := k >= 0 and from >= 0 and k > from
	var m: ShaderMaterial = warp.material
	m.set_shader_parameter("dir", 1.0 if outward else -1.0)
	m.set_shader_parameter("tint", hub.SCALES[k][2] if k >= 0 else Color(0.6, 0.7, 1.0))
	warp_title.text = ("Scale %d  ·  %s" % [k - 1, hub.SCALES[k][0]]) if k >= 0 else "The hub"
	warp_title.add_theme_color_override("font_color", hub.SCALES[k][2] if k >= 0 else Color(0.8, 0.9, 1.0))
	warp_sub.text = hub.SCALES[k][1] if k >= 0 else "Seven scales of one world."
	warp.visible = true
	var tw := create_tween()
	tw.tween_method(func(p: float) -> void: m.set_shader_parameter("p", p), 0.0, 1.0, 0.45).set_ease(Tween.EASE_IN)
	tw.parallel().tween_property(warp_title, "modulate:a", 1.0, 0.45)
	tw.parallel().tween_property(warp_sub, "modulate:a", 1.0, 0.45)
	tw.tween_callback(func() -> void:
		if k < 0:
			enter_hub()
		else:
			switch_to(k))
	tw.tween_interval(0.35)
	tw.tween_method(func(p: float) -> void: m.set_shader_parameter("p", p), 1.0, 0.0, 0.6).set_ease(Tween.EASE_OUT)
	tw.parallel().tween_property(warp_title, "modulate:a", 0.0, 0.8)
	tw.parallel().tween_property(warp_sub, "modulate:a", 0.0, 0.8)
	tw.tween_callback(func() -> void:
		warp.visible = false
		travelling = false)

func _build_bar() -> void:
	var layer := CanvasLayer.new()
	layer.layer = 9
	add_child(layer)
	bar = HBoxContainer.new()
	bar.anchor_left = 0.5
	bar.anchor_right = 0.5
	bar.offset_left = -440
	bar.offset_right = 440
	bar.offset_top = 4
	bar.alignment = BoxContainer.ALIGNMENT_CENTER
	bar.add_theme_constant_override("separation", 2)
	layer.add_child(bar)
	var home := Button.new()
	home.text = "◉ hub"
	home.flat = true
	home.focus_mode = Control.FOCUS_NONE
	home.add_theme_font_size_override("font_size", 12)
	home.pressed.connect(func() -> void: travel(-1))
	bar.add_child(home)
	for k in hub.SCALES.size():
		var b := Button.new()
		b.text = ("· " if k > 0 else "") + "%d %s" % [k - 1, hub.SCALES[k][0]]
		b.flat = true
		b.focus_mode = Control.FOCUS_NONE
		b.pressed.connect(func() -> void: travel(k))
		bar.add_child(b)
		bar_buttons.append(b)

func _build_warp() -> void:
	var layer := CanvasLayer.new()
	layer.layer = 20
	add_child(layer)
	warp = ColorRect.new()
	warp.set_anchors_preset(Control.PRESET_FULL_RECT)
	warp.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var m := ShaderMaterial.new()
	m.shader = load("res://shaders/warp.gdshader")
	warp.material = m
	warp.visible = false
	layer.add_child(warp)
	warp_title = Label.new()
	warp_title.add_theme_font_size_override("font_size", 44)
	warp_title.set_anchors_preset(Control.PRESET_CENTER)
	warp_title.offset_left = -500
	warp_title.offset_right = 500
	warp_title.offset_top = -40
	warp_title.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	warp_title.modulate.a = 0.0
	warp.add_child(warp_title)
	warp_sub = Label.new()
	warp_sub.add_theme_font_size_override("font_size", 18)
	warp_sub.add_theme_color_override("font_color", Color(0.8, 0.86, 0.95))
	warp_sub.set_anchors_preset(Control.PRESET_CENTER)
	warp_sub.offset_left = -500
	warp_sub.offset_right = 500
	warp_sub.offset_top = 20
	warp_sub.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	warp_sub.modulate.a = 0.0
	warp.add_child(warp_sub)

func _unhandled_input(e: InputEvent) -> void:
	if not (e is InputEventKey and e.pressed):
		return
	if e.keycode == KEY_M:
		travel(-1)
	elif e.keycode == KEY_TAB:
		if in_hub:
			travel(views.size() - 1 if e.shift_pressed else 0)
		else:
			travel(posmod(views.find(view) + (-1 if e.shift_pressed else 1), views.size()))

func _process(_d: float) -> void:
	if shot == "":
		return
	frames -= 1
	if frames == 0:
		var img := get_viewport().get_texture().get_image()
		img.save_png(shot)
		print("shot: ", shot)
		get_tree().quit()
