## Root: hosts one scale view at a time plus the HUD.
## Capture mode (no interaction, exits): godot --path godot -- --shot=out.png [--warm=SWEEPS] [--zoom=Z] [--focus=X,Y] [--frames=N]
extends Node

var view: Node
var views: Array = []
var hud: CanvasLayer
var shot := ""
var frames := 20

func _ready() -> void:
	for path in ["res://scripts/chem_view.gd", "res://scripts/pool_view.gd", "res://scripts/pond_view.gd", "res://scripts/cell_view.gd"]:
		var v: Node = load(path).new()
		add_child(v)
		views.append(v)
	view = views[0]
	for v in views:
		v.activate(v == view)
	hud = load("res://scripts/hud.gd").new()
	hud.view = view
	add_child(hud)
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
			"frames": frames = int(kv[1])
			"scale": switch_to(int(kv[1]))
			"pick": view.pick_longest()
			"warm": view.warm(int(kv[1]))
			"zoom": view.cam.zoom = Vector2(float(kv[1]), float(kv[1]))
			"focus":
				var xy := kv[1].split(",")
				view.cam.position = Vector2(float(xy[0]), float(xy[1])) * view.CELL

func switch_to(k: int) -> void:
	view = views[k % views.size()]
	for v in views:
		v.activate(v == view)
	hud.view = view

func _unhandled_input(e: InputEvent) -> void:
	if e is InputEventKey and e.pressed and e.keycode == KEY_TAB:
		switch_to(views.find(view) + 1)

func _process(_d: float) -> void:
	if shot == "":
		return
	frames -= 1
	if frames == 0:
		var img := get_viewport().get_texture().get_image()
		img.save_png(shot)
		print("shot: ", shot)
		get_tree().quit()
