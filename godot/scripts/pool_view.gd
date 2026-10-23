## Scale 1 view: tide pool of polymers. Chains drawn as their folds; click to inspect,
## click a residue in the close-up to see its atoms.
extends Node2D

const CELL := 6.0
const SPEEDS := [0, 1, 2, 4, 8]
const DT := 0.01
const EXPLAIN := """[b]Scale 1: chains (polymers)[/b]

[b]What you see.[/b] A tide pool of chains. Each chain has a backbone of Ka atoms (the only atom that bonds three ways) with one side group hanging off each link: Hy, Li, Bo, Ox or Fl. That makes every chain a [b]sequence of letters[/b], like a word.

[b]Folding.[/b] A chain curls up into a shape, driven by which side groups attract each other and which ones avoid water. The shape decides what the chain does: a pocket in the right place speeds up a chemical reaction (a [b]catalyst[/b], like an enzyme), oily chains form membranes, some absorb light (pigments).

[b]Copying.[/b] When the pool dries, loose letters stick to a chain's side groups by weak pairing, and a copy forms, letter by letter, with mistakes. Wet water melts copies apart. That is the first step to inheritance: sequences that copy better become more common.

[b]Real world.[/b] The "RNA world" idea: before DNA and proteins, chains like RNA may have both carried information and done chemistry.

[b]Try.[/b] Click a chain to see its fold; click a letter in the close-up to see its atoms."""
const SCALE_NAME := "scale 1: polymers"

var sim: PoolSim
var world: Node2D
var floor_rect: ColorRect
var lines: Node2D
var mm: MultiMesh
var res_node: MultiMeshInstance2D
var cam: Camera2D
var speed_idx := 2
var env := {}
var stats := {}
var selected := -1
var info := {}
var wet_smooth := 1.0
var dragging := false
var timer := 0.0
var inset: Control
var ok := false

func _ready() -> void:
	sim = PoolSim.new()
	add_child(sim)
	ok = sim.start(ProjectSettings.globalize_path("res://../data/rates0.json"), 1)
	var area: Vector2 = sim.area()

	world = Node2D.new()
	world.scale = Vector2(CELL, CELL)
	add_child(world)
	floor_rect = ColorRect.new()
	floor_rect.size = area
	floor_rect.position = Vector2(-2, -2)
	floor_rect.size = area + Vector2(4, 4)
	var fm := ShaderMaterial.new()
	fm.shader = load("res://shaders/pool.gdshader")
	fm.set_shader_parameter("area", area)
	floor_rect.material = fm
	floor_rect.mouse_filter = Control.MOUSE_FILTER_IGNORE
	world.add_child(floor_rect)

	lines = Node2D.new()
	lines.draw.connect(_draw_lines)
	world.add_child(lines)

	mm = MultiMesh.new()
	mm.transform_format = MultiMesh.TRANSFORM_2D
	mm.use_colors = true
	mm.use_custom_data = true
	var quad := QuadMesh.new()
	quad.size = Vector2(1.0, 1.0)
	mm.mesh = quad
	res_node = MultiMeshInstance2D.new()
	res_node.multimesh = mm
	var am := ShaderMaterial.new()
	am.shader = load("res://shaders/atom.gdshader")
	am.set_shader_parameter("palette", load("res://scripts/chem_view.gd").palette())
	res_node.material = am
	world.add_child(res_node)

	cam = Camera2D.new()
	add_child(cam)
	cam.position = area * CELL * 0.5
	var vs := get_viewport_rect().size
	var fit: float = min(vs.x / (area.x * CELL), vs.y / (area.y * CELL)) * 0.95
	cam.zoom = Vector2(fit, fit)

	var layer := CanvasLayer.new()
	layer.layer = 2
	add_child(layer)
	inset = load("res://scripts/fold_inset.gd").new()
	inset.position = Vector2(12, 430)
	inset.size = Vector2(330, 500)
	inset.visible = false
	layer.add_child(inset)
	if ok:
		sim.step(1, DT)

func activate(on: bool) -> void:
	visible = on
	set_process(on)
	set_process_unhandled_input(on)
	inset.visible = on and not info.is_empty()
	if on:
		cam.make_current()

func speed() -> int:
	return SPEEDS[speed_idx]

func px_per_cell() -> float:
	return CELL * cam.zoom.x

func _process(delta: float) -> void:
	if not ok:
		return
	if speed() > 0:
		sim.step(speed(), DT)
	env = sim.env()
	wet_smooth = lerp(wet_smooth, 1.0 if env.wet else 0.0, clamp(delta * 1.5, 0.0, 1.0))
	var light: float = clamp(env.visible * 1.6 + 0.12, 0.0, 1.0)
	var buf: PackedFloat32Array = sim.instance_buffer(selected)
	mm.instance_count = buf.size() / 16
	if buf.size() > 0:
		mm.buffer = buf
	var am: ShaderMaterial = res_node.material
	am.set_shader_parameter("px_per_cell", px_per_cell())
	am.set_shader_parameter("light", light)
	var fm: ShaderMaterial = floor_rect.material
	fm.set_shader_parameter("wet", wet_smooth)
	fm.set_shader_parameter("light", light)
	lines.queue_redraw()
	timer -= delta
	if timer <= 0.0:
		timer = 0.4
		stats = sim.stats()
		if selected >= 0:
			info = sim.chain_info(selected)
			if info.is_empty():
				selected = -1
			elif info.id != selected:
				selected = info.id
		inset.show_chain(info)
		inset.visible = not info.is_empty()

func _draw_lines() -> void:
	var l: PackedVector2Array = sim.backbone_lines()
	if l.size() > 0:
		lines.draw_multiline(l, Color(0.85, 0.85, 0.8, 0.55), 0.18)

func selected_info() -> Dictionary:
	return {}

## Capture helper: select the longest chain and one of its residues.
func pick_longest() -> void:
	var best := -1
	var best_len := 0
	var area: Vector2 = sim.area()
	for gx in range(0, int(area.x), 4):
		for gy in range(0, int(area.y), 4):
			var id: int = sim.chain_at(Vector2(gx, gy), 3.0)
			if id >= 0:
				var d: Dictionary = sim.chain_info(id)
				if d.letters.size() > best_len:
					best_len = d.letters.size()
					best = id
	selected = best
	info = sim.chain_info(best) if best >= 0 else {}
	inset.show_chain(info)
	inset.visible = not info.is_empty()
	inset.residue = 2 if best_len > 2 else -1

## Campaign goal: a chain lineage this many copies deep.
const GOAL_GEN := 5
func goal_progress() -> Dictionary:
	var v: int = stats.get("max_generation", 0)
	return {"value": v, "p": clamp(float(v) / GOAL_GEN, 0.0, 1.0), "done": v >= GOAL_GEN, "args": [GOAL_GEN], "now": "deepest copy lineage %d" % v}

func hud_text() -> String:
	if not ok:
		return "[color=#f88]data/rates0.json missing: run primordium train[/color]"
	var s := stats
	if s.is_empty():
		return ""
	var t := "[b]Pool[/b] (%s)\nchains %d   mean len %.1f   max %d\ngeneration %d   copies %d   copy error %.0f%%\nfree monomers %d\nbackbone catalysis %.2f kT (best ever %.2f)\n" % [
		"flooded" if env.get("wet", false) else "drying", s.chains, s.mean_len, s.max_len, s.max_generation, s.copies, s.copy_error * 100.0, s.free_monomers, s.backbone_cut, s.best_cut]
	t += "[b]Function surrogate[/b]  folds %d   exact %d   neighbour %d\n" % [s.folds, s.exact, s.nearest]
	t += "[b]Discoveries[/b]\n"
	for d in s.discoveries:
		t += "[color=#cbf]" + d + "[/color]\n"
	if not info.is_empty():
		t += "\n[b]Chain %d[/b]  gen %d  age %.1f d  %s\n[color=#dde]%s[/color]\nstability %.2f  amphiphile %.2f  pigment %.2f  conf %.2f\n" % [
			info.id, info.generation, info.age_days, "paired" if info.paired else "free", info.seq, info.stability, info.amphiphile, info.pigment, info.confidence]
		t += "catalysis: " + ", ".join(PackedStringArray(info.catalysis.slice(0, 4))) + "\n"
	t += "\n[color=#789]Tab: switch scale   click a chain to inspect\nclick a residue in the close-up for its atoms[/color]"
	return t

func screen_to_world(sp: Vector2) -> Vector2:
	var vs := get_viewport_rect().size
	return (cam.position + (sp - vs * 0.5) / cam.zoom) / CELL

func zoom_at(factor: float, sp: Vector2) -> void:
	var vs := get_viewport_rect().size
	var before := cam.position + (sp - vs * 0.5) / cam.zoom
	var z: float = clamp(cam.zoom.x * factor, 0.3, 20.0)
	cam.zoom = Vector2(z, z)
	cam.position = before - (sp - vs * 0.5) / cam.zoom

func _unhandled_input(e: InputEvent) -> void:
	if e is InputEventMouseButton:
		if e.button_index == MOUSE_BUTTON_WHEEL_UP and e.pressed:
			zoom_at(1.15, e.position)
		elif e.button_index == MOUSE_BUTTON_WHEEL_DOWN and e.pressed:
			zoom_at(1.0 / 1.15, e.position)
		elif e.button_index in [MOUSE_BUTTON_RIGHT, MOUSE_BUTTON_MIDDLE]:
			dragging = e.pressed
		elif e.button_index == MOUSE_BUTTON_LEFT and e.pressed:
			selected = sim.chain_at(screen_to_world(e.position), 6.0)
			info = sim.chain_info(selected) if selected >= 0 else {}
			inset.show_chain(info)
			inset.visible = not info.is_empty()
	elif e is InputEventMouseMotion and dragging:
		cam.position -= e.relative / cam.zoom
	elif e is InputEventKey and e.pressed:
		match e.keycode:
			KEY_SPACE:
				speed_idx = 0 if speed_idx != 0 else 2
			KEY_1, KEY_2, KEY_3, KEY_4, KEY_5:
				speed_idx = e.keycode - KEY_1

func warm(n: int) -> void:
	sim.step(n, DT)
