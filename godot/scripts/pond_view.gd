## Scale 2 view: tide pond of protocells. Membranes are bilayer rings sized by their
## residues; dots inside are the vesicle's own chains. Click a vesicle for its interior,
## click a chain thumbnail for its fold, click a residue for its atoms.
extends Node2D

const CELL := 6.0
const SPEEDS := [0, 1, 2, 4, 8]
const DT := 0.01
const EXPLAIN := """[b]Scale 2: protocells[/b]

[b]What you see.[/b] A pond with bubbles (vesicles). Each ring is a membrane made of oily chains; the dots inside are the chains it has trapped.

[b]Why bubbles matter.[/b] Oily chains in water clump into closed shells on their own, like soap bubbles. A shell keeps its chains together, so a chain that makes something useful helps [b]its own[/b] bubble, not the whole pond. Bubbles grow by soaking up more oily chains, get stretched by the tide and split in two, and can burst.

[b]What drives change.[/b] Tides feed the pond from the ocean and wash it out; UV light erodes membranes (pigments inside give shade); the day's warmth changes how fast everything happens.

[b]Real world.[/b] Fatty-acid vesicles that grow and divide are studied in labs as models of the first cells (protocells).

[b]Try.[/b] Click a bubble to look inside, then a chain for its fold, then a letter for its atoms."""
const SCALE_NAME := "scale 2: protocells"

var sim: PondSim
var world: Node2D
var floor_rect: ColorRect
var art: Node2D
var cam: Camera2D
var speed_idx := 2
var env := {}
var stats := {}
var vesicles: Array = []
var specks := PackedFloat32Array()
var selected := -1
var interior: Array = []
var wet_smooth := 1.0
var dragging := false
var timer := 0.0
var thumbs: Control
var inset: Control
var ok := false
var palette: Array = []
var auto := true

func _ready() -> void:
	palette = load("res://scripts/chem_view.gd").PALETTE
	sim = PondSim.new()
	add_child(sim)
	ok = sim.start(ProjectSettings.globalize_path("res://../data/rates0.json"), 1, auto)
	var area: Vector2 = sim.area()
	world = Node2D.new()
	world.scale = Vector2(CELL, CELL)
	add_child(world)
	floor_rect = ColorRect.new()
	floor_rect.position = Vector2(-2, -2)
	floor_rect.size = area + Vector2(4, 4)
	var fm := ShaderMaterial.new()
	fm.shader = load("res://shaders/pool.gdshader")
	fm.set_shader_parameter("area", area)
	floor_rect.material = fm
	floor_rect.mouse_filter = Control.MOUSE_FILTER_IGNORE
	world.add_child(floor_rect)
	art = Node2D.new()
	art.draw.connect(_draw_pond)
	world.add_child(art)
	cam = Camera2D.new()
	add_child(cam)
	cam.position = area * CELL * 0.5
	var vs := get_viewport_rect().size
	var fit: float = min(vs.x / (area.x * CELL), vs.y / (area.y * CELL)) * 0.95
	cam.zoom = Vector2(fit, fit)
	var layer := CanvasLayer.new()
	layer.layer = 2
	add_child(layer)
	thumbs = Control.new()
	thumbs.position = Vector2(12, 430)
	thumbs.size = Vector2(330, 150)
	thumbs.mouse_filter = Control.MOUSE_FILTER_STOP
	thumbs.draw.connect(_draw_thumbs)
	thumbs.gui_input.connect(_thumb_input)
	layer.add_child(thumbs)
	inset = load("res://scripts/fold_inset.gd").new()
	inset.position = Vector2(12, 588)
	inset.size = Vector2(330, 360)
	inset.fold_h = 200.0
	inset.visible = false
	layer.add_child(inset)
	if ok:
		sim.step(1, DT)

func activate(on: bool) -> void:
	visible = on
	set_process(on)
	set_process_unhandled_input(on)
	thumbs.visible = on and selected >= 0
	inset.visible = on and not inset.info.is_empty()
	if on:
		cam.make_current()

func speed() -> int:
	return SPEEDS[speed_idx]

func warm(n: int) -> void:
	sim.step(n, DT)

func selected_info() -> Dictionary:
	return {}

func _process(delta: float) -> void:
	if not ok:
		return
	if speed() > 0:
		sim.step(speed(), DT)
	env = sim.env()
	vesicles = sim.vesicles()
	specks = sim.specks()
	wet_smooth = lerp(wet_smooth, 1.0 if env.wet else 0.0, clamp(delta * 1.5, 0.0, 1.0))
	var light: float = clamp(env.visible * 1.6 + 0.12, 0.0, 1.0)
	var fm: ShaderMaterial = floor_rect.material
	fm.set_shader_parameter("wet", wet_smooth)
	fm.set_shader_parameter("light", light)
	art.modulate = Color(1, 1, 1).lerp(Color(0.45, 0.5, 0.75), 1.0 - light)
	art.queue_redraw()
	timer -= delta
	if timer <= 0.0:
		timer = 0.4
		stats = sim.stats()
		if selected >= 0:
			selected = sim.follow(selected)
			interior = sim.interior(selected) if selected >= 0 else []
			if not inset.info.is_empty():
				var still := interior.filter(func(c): return c.id == inset.info.id)
				if still.size() > 0:
					inset.show_chain(still[0])
		thumbs.visible = selected >= 0
		thumbs.queue_redraw()

## Bilayer membrane: two wobbling rings. Wobble phase from id so each cell keeps its shape.
func _membrane(c: Vector2, r: float, id: int, col: Color, width: float) -> void:
	var pts := PackedVector2Array()
	var t := Time.get_ticks_msec() / 1000.0
	var n := 40
	for k in n + 1:
		var a := TAU * k / n
		var wob := 0.06 * sin(a * 3.0 + id * 1.7 + t * 0.8) + 0.04 * sin(a * 5.0 - id + t * 1.3)
		pts.append(c + Vector2(cos(a), sin(a)) * r * (1.0 + wob))
	art.draw_polyline(pts, col, width, true)

func _draw_pond() -> void:
	for k in range(0, specks.size(), 3):
		art.draw_circle(Vector2(specks[k], specks[k + 1]), 0.28, Color(palette[int(specks[k + 2])], 0.55))
	for v in vesicles:
		var c: Vector2 = v.pos
		var r: float = v.r
		var amber := Color(0.95, 0.65, 0.2)
		var fill := Color(0.35, 0.75, 0.85, 0.16).lerp(Color(amber, 0.3), clamp(v.pigment * 1.5, 0.0, 1.0))
		art.draw_circle(c, r, fill)
		var z: PackedFloat32Array = v.chain_z
		for k in z.size():
			# golden-angle spiral packs interior chains
			var a: float = k * 2.39996 + v.id * 0.3
			var rr: float = r * 0.8 * sqrt((k + 0.5) / max(z.size(), 1))
			art.draw_circle(c + Vector2(cos(a), sin(a)) * rr, 0.35, palette[int(z[k])])
		var mem := Color(0.95, 0.9, 0.75, 0.85)
		if v.surrogate:
			mem = Color(0.75, 0.6, 1.0, 0.85)
		_membrane(c, r, v.id, mem, 0.22)
		_membrane(c, r * 0.9, v.id, Color(mem, 0.45), 0.12)
		if v.id == selected:
			art.draw_arc(c, r + 1.2, 0, TAU, 48, Color(1, 1, 0.4, 0.9), 0.25)

## Campaign goal: protocell lineage this many divisions deep.
const GOAL_GEN := 4
func goal_progress() -> Dictionary:
	var v: int = stats.get("max_generation", 0)
	return {"value": v, "p": clamp(float(v) / GOAL_GEN, 0.0, 1.0), "done": v >= GOAL_GEN, "args": [GOAL_GEN], "now": "deepest protocell lineage %d" % v}

func hud_text() -> String:
	if not ok:
		return "[color=#f88]data/rates0.json missing: run primordium train[/color]"
	var s := stats
	if s.is_empty():
		return ""
	var t := "[b]Pond[/b] (%s)\nvesicles %d (surrogate %d)   generation %d\nformed %d   divided %d   burst %d\nfree chains in pond %d\n" % [
		"flooded" if env.get("wet", false) else "drying", s.vesicles, s.surrogate, s.max_generation, s.nucleations, s.divisions, s.bursts, s.outside_chains]
	t += "backbone catalysis inside %.2f / pond %.2f kT\ncopy error inside %.0f%% / pond %.0f%%\n" % [s.cut_in, s.cut_out, s.err_in * 100.0, s.err_out * 100.0]
	t += "[b]Resolution[/b] %s   lifts %d\n" % ["auto (A)" if s.auto else "detail (A)", s.lifts]
	t += "[b]Discoveries[/b]\n"
	for d in s.discoveries:
		t += "[color=#cbf]" + d + "[/color]\n"
	if selected >= 0:
		for v in vesicles:
			if v.id == selected:
				t += "\n[b]Vesicle %d[/b] gen %d  %s\nmembrane %d residues   chains %d   monomers %d\ncatalysis %.2f kT   pigment %.2f   osmotic %.3f\n" % [
					v.id, v.generation, "surrogate %.2f" % v.confidence if v.surrogate else "detail", v.area, v.chains, v.free, v.cut, v.pigment, v.osmotic]
	t += "\n[color=#789]Tab: switch scale   A: auto resolution\nclick a vesicle, then a chain, then a residue[/color]"
	return t

func _thumb_layout() -> Array:
	var cells := []
	var cols := 8
	for k in min(interior.size(), 24):
		cells.append(Rect2(8 + (k % cols) * 40, 24 + (k / cols) * 40, 36, 36))
	return cells

func _draw_thumbs() -> void:
	if selected < 0:
		return
	var font := ThemeDB.fallback_font
	thumbs.draw_rect(Rect2(Vector2.ZERO, thumbs.size), Color(0.03, 0.05, 0.08, 0.86))
	thumbs.draw_string(font, Vector2(8, 16), "Vesicle %d interior: %d chains (click one)" % [selected, interior.size()], HORIZONTAL_ALIGNMENT_LEFT, -1, 12, Color(1, 1, 1, 0.8))
	var cells := _thumb_layout()
	for k in cells.size():
		var box: Rect2 = cells[k]
		var ch: Dictionary = interior[k]
		var hl: bool = not inset.info.is_empty() and inset.info.id == ch.id
		thumbs.draw_rect(box, Color(1, 1, 0.4, 0.25) if hl else Color(1, 1, 1, 0.05))
		var fold: PackedVector2Array = ch.fold
		var lo := Vector2(1e9, 1e9)
		var hi := Vector2(-1e9, -1e9)
		for p in fold:
			lo = lo.min(p)
			hi = hi.max(p)
		var span: Vector2 = hi - lo + Vector2.ONE
		var s: float = min(box.size.x / span.x, box.size.y / span.y) * 0.9
		var off: Vector2 = box.position + (box.size - span * s) * 0.5 - lo * s + Vector2(s, s) * 0.5
		var letters: PackedByteArray = ch.letters
		for i in fold.size():
			thumbs.draw_circle(fold[i] * s + off, max(s * 0.35, 1.0), palette[[1, 3, 4, 6, 7][letters[i]]])

func _thumb_input(e: InputEvent) -> void:
	if not (e is InputEventMouseButton and e.pressed and e.button_index == MOUSE_BUTTON_LEFT):
		return
	var cells := _thumb_layout()
	for k in cells.size():
		if cells[k].has_point(e.position):
			inset.show_chain(interior[k])
			inset.visible = true
			thumbs.queue_redraw()
			thumbs.accept_event()
			return

func screen_to_world(sp: Vector2) -> Vector2:
	var vs := get_viewport_rect().size
	return (cam.position + (sp - vs * 0.5) / cam.zoom) / CELL

func zoom_at(factor: float, sp: Vector2) -> void:
	var vs := get_viewport_rect().size
	var before := cam.position + (sp - vs * 0.5) / cam.zoom
	var z: float = clamp(cam.zoom.x * factor, 0.3, 20.0)
	cam.zoom = Vector2(z, z)
	cam.position = before - (sp - vs * 0.5) / cam.zoom

## Capture helper: select the vesicle with the most chains, open its first chain and a residue.
func pick_longest() -> void:
	var best := -1
	var most := -1
	for v in vesicles if not vesicles.is_empty() else sim.vesicles():
		if v.chains > most:
			most = v.chains
			best = v.id
	selected = best
	interior = sim.interior(best) if best >= 0 else []
	var longest := {}
	for ch in interior:
		if longest.is_empty() or ch.letters.size() > longest.letters.size():
			longest = ch
	if not longest.is_empty():
		inset.show_chain(longest)
		inset.residue = 1
		inset.visible = true
	thumbs.visible = selected >= 0

func _unhandled_input(e: InputEvent) -> void:
	if e is InputEventMouseButton:
		if e.button_index == MOUSE_BUTTON_WHEEL_UP and e.pressed:
			zoom_at(1.15, e.position)
		elif e.button_index == MOUSE_BUTTON_WHEEL_DOWN and e.pressed:
			zoom_at(1.0 / 1.15, e.position)
		elif e.button_index in [MOUSE_BUTTON_RIGHT, MOUSE_BUTTON_MIDDLE]:
			dragging = e.pressed
		elif e.button_index == MOUSE_BUTTON_LEFT and e.pressed:
			selected = sim.vesicle_at(screen_to_world(e.position))
			interior = sim.interior(selected) if selected >= 0 else []
			inset.show_chain({})
			inset.visible = false
			thumbs.visible = selected >= 0
			thumbs.queue_redraw()
	elif e is InputEventMouseMotion and dragging:
		cam.position -= e.relative / cam.zoom
	elif e is InputEventKey and e.pressed:
		match e.keycode:
			KEY_SPACE:
				speed_idx = 0 if speed_idx != 0 else 2
			KEY_1, KEY_2, KEY_3, KEY_4, KEY_5:
				speed_idx = e.keycode - KEY_1
			KEY_A:
				auto = not auto
				sim.set_auto(auto)
