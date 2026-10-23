## Root: hosts one scale view at a time plus the HUD.
## Capture mode (no interaction, exits): godot --path godot -- --shot=out.png [--warm=SWEEPS] [--zoom=Z] [--focus=X,Y] [--frames=N]
extends Node

var view: Node
var hud: CanvasLayer
var shot := ""
var frames := 20

func _ready() -> void:
	view = load("res://scripts/chem_view.gd").new()
	add_child(view)
	hud = load("res://scripts/hud.gd").new()
	hud.view = view
	add_child(hud)
	for a in OS.get_cmdline_user_args():
		var kv: PackedStringArray = a.trim_prefix("--").split("=")
		if kv.size() != 2:
			continue
		match kv[0]:
			"shot": shot = kv[1]
			"frames": frames = int(kv[1])
			"warm": view.sim.step(int(kv[1]))
			"zoom": view.cam.zoom = Vector2(float(kv[1]), float(kv[1]))
			"focus":
				var xy := kv[1].split(",")
				view.cam.position = Vector2(float(xy[0]), float(xy[1])) * view.CELL

func _process(_d: float) -> void:
	if shot == "":
		return
	frames -= 1
	if frames == 0:
		var img := get_viewport().get_texture().get_image()
		img.save_png(shot)
		print("shot: ", shot)
		get_tree().quit()
