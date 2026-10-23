## Scale 1 view: tide pool of polymers. Chains drawn as their folds; click to inspect,
## click a residue in the close-up to see its atoms.
extends Node2D

const CELL := 6.0
const SPEEDS := [0, 1, 2, 4, 8]
const DT := 0.01
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
var residue := -1
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
	am.set_shader_parameter("palette", load("res://scripts/chem_view.gd").PALETTE)
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
	inset = Control.new()
	inset.position = Vector2(12, 430)
	inset.size = Vector2(330, 500)
	inset.mouse_filter = Control.MOUSE_FILTER_STOP
	inset.draw.connect(_draw_inset)
	inset.gui_input.connect(_inset_input)
	layer.add_child(inset)
	if ok:
		sim.step(1, DT)

func activate(on: bool) -> void:
	visible = on
	set_process(on)
	set_process_unhandled_input(on)
	inset.visible = on
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
	if residue >= 0:
		inset.queue_redraw()
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
				residue = min(residue, info.letters.size() - 1)
		inset.queue_redraw()

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
	residue = 2 if best_len > 2 else -1

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
			residue = -1
			info = sim.chain_info(selected) if selected >= 0 else {}
			inset.queue_redraw()
	elif e is InputEventMouseMotion and dragging:
		cam.position -= e.relative / cam.zoom
	elif e is InputEventKey and e.pressed:
		match e.keycode:
			KEY_SPACE:
				speed_idx = 0 if speed_idx != 0 else 2
			KEY_1, KEY_2, KEY_3, KEY_4, KEY_5:
				speed_idx = e.keycode - KEY_1

## Close-up: selected chain's fold, letters, catalytic sites; then one residue's atoms.
func _inset_layout() -> Dictionary:
	var fold: PackedVector2Array = info.fold
	var lo := Vector2(1e9, 1e9)
	var hi := Vector2(-1e9, -1e9)
	for p in fold:
		lo = lo.min(p)
		hi = hi.max(p)
	var span: Vector2 = (hi - lo) + Vector2.ONE
	var box := Rect2(10, 30, 310, 230)
	var s: float = min(box.size.x / span.x, box.size.y / span.y)
	s = min(s, 34.0)
	var off: Vector2 = box.position + (box.size - span * s) * 0.5 - lo * s + Vector2(s, s) * 0.5
	return {"s": s, "off": off}

func _inset_input(e: InputEvent) -> void:
	if not (e is InputEventMouseButton and e.pressed and e.button_index == MOUSE_BUTTON_LEFT) or info.is_empty():
		return
	var lay := _inset_layout()
	var fold: PackedVector2Array = info.fold
	for k in fold.size():
		if (fold[k] * lay.s + lay.off).distance_to(e.position) < lay.s * 0.5:
			residue = k
			inset.queue_redraw()
			return

func _draw_inset() -> void:
	if info.is_empty():
		return
	var pal: Array = load("res://scripts/chem_view.gd").PALETTE
	var font := ThemeDB.fallback_font
	inset.draw_rect(Rect2(0, 0, 330, 500), Color(0.03, 0.05, 0.08, 0.86))
	inset.draw_string(font, Vector2(10, 20), "Fold close-up (dashes = catalytic pockets)", HORIZONTAL_ALIGNMENT_LEFT, -1, 12, Color(1, 1, 1, 0.8))
	var lay := _inset_layout()
	var fold: PackedVector2Array = info.fold
	var letters: PackedByteArray = info.letters
	var s: float = lay.s
	for k in fold.size() - 1:
		inset.draw_line(fold[k] * s + lay.off, fold[k + 1] * s + lay.off, Color(0.9, 0.9, 0.85, 0.8), 3.0)
	var sites: PackedVector2Array = info.sites
	for k in range(0, sites.size(), 2):
		inset.draw_dashed_line(sites[k] * s + lay.off, sites[k + 1] * s + lay.off, Color(1.0, 0.85, 0.3, 0.9), 2.0, 4.0)
	var zs := [1, 3, 4, 6, 7]
	for k in fold.size():
		var c: Vector2 = fold[k] * s + lay.off
		var z: int = zs[letters[k]]
		inset.draw_circle(c, s * 0.32, pal[z])
		if k == residue:
			inset.draw_arc(c, s * 0.45, 0, TAU, 24, Color(1, 1, 0.4), 2.0)
		inset.draw_string(font, c + Vector2(-4, 4), "hlbof"[letters[k]], HORIZONTAL_ALIGNMENT_LEFT, -1, 11, Color(0, 0, 0, 0.8))
	if residue < 0 or residue >= letters.size():
		inset.draw_string(font, Vector2(10, 290), "click a residue to see its atoms", HORIZONTAL_ALIGNMENT_LEFT, -1, 12, Color(1, 1, 1, 0.5))
		return
	# scale 0 inside scale 1: the residue's atoms with their shells
	var letter: int = letters[residue]
	inset.draw_string(font, Vector2(10, 290), "Residue %d: Ka backbone + %s side group" % [residue, sim.letter_name(letter)], HORIZONTAL_ALIGNMENT_LEFT, -1, 12, Color(1, 1, 1, 0.8))
	var atoms: Array = sim.residue_atoms(letter)
	var names := ["", "Hy", "Ne", "Li", "Bo", "Ka", "Ox", "Fl", "Ar"]
	var t := Time.get_ticks_msec() / 1000.0
	for k in atoms.size():
		var a: Dictionary = atoms[k]
		var c: Vector2 = Vector2(165, 330) + a.pos * 62.0
		if k > 0:
			inset.draw_line(Vector2(165, 330) + atoms[k - 1].pos * 62.0, c, Color(1, 1, 1, 0.7), 3.0)
	for k in atoms.size():
		var a: Dictionary = atoms[k]
		var c: Vector2 = Vector2(165, 330) + a.pos * 62.0
		var z: int = a.z
		var col: Color = pal[z]
		inset.draw_circle(c, 26, Color(col, 0.18))
		inset.draw_circle(c, 5, col.lerp(Color.WHITE, 0.4))
		inset.draw_arc(c, 12, 0, TAU, 24, Color(col, 0.6), 1.0)
		for e in min(z, 2):
			var ang: float = t * 2.3 + PI * e + k
			inset.draw_circle(c + Vector2(cos(ang), sin(ang)) * 12, 2.2, Color(0.6, 0.85, 1.0))
		var outer: int = max(z - 2, 0)
		if outer > 0:
			inset.draw_arc(c, 22, 0, TAU, 32, Color(col, 0.6), 1.0)
		for e in outer:
			var ang: float = -t * 1.1 + TAU * e / outer + k
			var shared: bool = e < a.bonds
			inset.draw_circle(c + Vector2(cos(ang), sin(ang)) * 22, 2.6, Color.WHITE if shared else Color(0.55, 0.8, 1.0))
		inset.draw_string(font, c + Vector2(28, 5), "%s (Z=%d)" % [names[z], z], HORIZONTAL_ALIGNMENT_LEFT, -1, 12, Color(1, 1, 1, 0.75))

func warm(n: int) -> void:
	sim.step(n, DT)
