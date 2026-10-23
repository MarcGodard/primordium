## Scale 3 view: cells on the beach. Left to right = sea to dune (one band per patch).
## Cell tint = what its genes do: teal polymerase, green metabolism, cream membrane, amber pigment.
## Zoom in to see each genome ring; click a cell for its genome, a gene for its product's fold.
extends Node2D

const CELL := 6.0
const SPEEDS := [0, 1, 2, 4, 8]
const DT := 0.02
const SCALE_NAME := "scale 3: cells"
const BAND := 22.0
const HEIGHT := 140.0
const ROLE_COLORS := [Color(0.3, 0.85, 0.85), Color(0.45, 0.85, 0.4), Color(0.95, 0.9, 0.75), Color(0.95, 0.65, 0.2), Color(0.6, 0.6, 0.65)]
const ROLE_NAMES := ["polymerase", "metabolism", "membrane", "pigment", "no clear role"]

var sim: CellSim
var world: Node2D
var beach: ColorRect
var art: Node2D
var cam: Camera2D
var speed_idx := 2
var env := {}
var stats := {}
var cells: Array = []
var patches: Array = []
var selected := -1
var detail := {}
var gene_sel := -1
var dragging := false
var timer := 0.0
var genome_panel: Control
var chart: Control
var inset: Control
var ok := false
var auto := true
var history := PackedFloat32Array()

func _ready() -> void:
	sim = CellSim.new()
	add_child(sim)
	ok = sim.start(ProjectSettings.globalize_path("res://../data/rates0.json"), 1, auto)
	world = Node2D.new()
	world.scale = Vector2(CELL, CELL)
	add_child(world)
	beach = ColorRect.new()
	beach.size = Vector2(BAND * 10.0, HEIGHT)
	var bm := ShaderMaterial.new()
	bm.shader = load("res://shaders/beach.gdshader")
	bm.set_shader_parameter("grid", Vector2(BAND * 10.0, HEIGHT))
	bm.set_shader_parameter("elev_range", Vector2(0.28, 0.77))
	beach.material = bm
	beach.mouse_filter = Control.MOUSE_FILTER_IGNORE
	world.add_child(beach)
	art = Node2D.new()
	art.draw.connect(_draw_cells)
	world.add_child(art)
	cam = Camera2D.new()
	add_child(cam)
	cam.position = Vector2(BAND * 5.0, HEIGHT * 0.5) * CELL
	var vs := get_viewport_rect().size
	var fit: float = min(vs.x / (BAND * 10.0 * CELL), vs.y / (HEIGHT * CELL)) * 0.95
	cam.zoom = Vector2(fit, fit)
	var layer := CanvasLayer.new()
	layer.layer = 2
	add_child(layer)
	genome_panel = Control.new()
	genome_panel.position = Vector2(12, 430)
	genome_panel.size = Vector2(330, 250)
	genome_panel.mouse_filter = Control.MOUSE_FILTER_STOP
	genome_panel.draw.connect(_draw_genome)
	genome_panel.gui_input.connect(_genome_input)
	genome_panel.visible = false
	layer.add_child(genome_panel)
	inset = load("res://scripts/fold_inset.gd").new()
	inset.position = Vector2(12, 688)
	inset.size = Vector2(330, 262)
	inset.fold_h = 150.0
	inset.visible = false
	layer.add_child(inset)
	chart = Control.new()
	chart.anchor_left = 0.5
	chart.anchor_right = 0.5
	chart.anchor_top = 1.0
	chart.anchor_bottom = 1.0
	chart.offset_left = -300
	chart.offset_right = 300
	chart.offset_top = -132
	chart.offset_bottom = -12
	chart.mouse_filter = Control.MOUSE_FILTER_IGNORE
	chart.draw.connect(_draw_chart)
	layer.add_child(chart)
	if ok:
		sim.step(1, DT)

func activate(on: bool) -> void:
	visible = on
	set_process(on)
	set_process_unhandled_input(on)
	genome_panel.visible = on and not detail.is_empty()
	inset.visible = on and not inset.info.is_empty()
	chart.visible = on
	if on:
		cam.make_current()

func speed() -> int:
	return SPEEDS[speed_idx]

func warm(n: int) -> void:
	sim.step(n, DT)

func selected_info() -> Dictionary:
	return {}

func px_per_unit() -> float:
	return CELL * cam.zoom.x

func _process(delta: float) -> void:
	if not ok:
		return
	if speed() > 0:
		sim.step(speed(), DT)
	env = sim.env()
	cells = sim.cells()
	patches = sim.patches()
	var light: float = clamp(env.visible * 1.6 + 0.12, 0.0, 1.0)
	var bm: ShaderMaterial = beach.material
	bm.set_shader_parameter("tide_level", env.tide_level)
	bm.set_shader_parameter("light", light)
	bm.set_shader_parameter("uv_glow", clamp(env.flare * 0.4, 0.0, 0.5))
	art.modulate = Color(1, 1, 1).lerp(Color(0.5, 0.55, 0.8), 1.0 - light)
	art.queue_redraw()
	timer -= delta
	if timer <= 0.0:
		timer = 0.4
		stats = sim.stats()
		history = sim.history()
		chart.queue_redraw()
		if selected >= 0:
			selected = sim.follow(selected)
			detail = sim.cell_detail(selected) if selected >= 0 else {}
			if gene_sel >= 0 and gene_sel < detail.get("genes", []).size():
				inset.show_chain(detail.genes[gene_sel])
		genome_panel.visible = not detail.is_empty()
		genome_panel.queue_redraw()

func cell_pos(c: Dictionary) -> Vector2:
	var jitter: float = fmod(float(c.id) * 0.618, 1.0)
	return Vector2((float(c.patch) + 0.15 + 0.7 * jitter) * BAND, float(c.y) * HEIGHT)

func cell_tint(c: Dictionary) -> Color:
	var w := [c.poly / 0.5, c.synth / 0.5, c.amph / 0.4, c.pigment / 0.8]
	var col := Color(0, 0, 0, 0)
	var tot := 0.0
	for k in 4:
		col += ROLE_COLORS[k] * w[k]
		tot += w[k]
	return (col / max(tot, 0.001)) if tot > 0.01 else ROLE_COLORS[4]

func _draw_cells() -> void:
	var t := Time.get_ticks_msec() / 1000.0
	var font := ThemeDB.fallback_font
	var near := px_per_unit() > 20.0
	for k in patches.size():
		var p: Dictionary = patches[k]
		art.draw_line(Vector2(k * BAND, 0), Vector2(k * BAND, HEIGHT), Color(1, 1, 1, 0.08), 0.15)
	for c in cells:
		var pos := cell_pos(c)
		var r: float = c.r
		var tint := cell_tint(c)
		art.draw_circle(pos, r, Color(tint, 0.28))
		var pts := PackedVector2Array()
		for i in 25:
			var a := TAU * i / 24.0
			var wob := 0.07 * sin(a * 3.0 + c.id * 1.3 + t * 0.9)
			pts.append(pos + Vector2(cos(a), sin(a)) * r * (1.0 + wob))
		var mem := Color(0.75, 0.6, 1.0, 0.9) if c.surrogate else Color(tint.lightened(0.3), 0.9)
		art.draw_polyline(pts, mem, 0.18 if not near else 0.08)
		# genome ring; the replication fork grows a second ring as it copies
		art.draw_arc(pos, r * 0.45, 0, TAU, 20, Color(0.95, 0.95, 1.0, 0.55), 0.12 if not near else 0.05)
		if c.fork > 0.0:
			art.draw_arc(pos, r * 0.6, -PI / 2, -PI / 2 + TAU * c.fork, 20, Color(1.0, 0.9, 0.4, 0.8), 0.1 if not near else 0.05)
		if c.id == selected:
			art.draw_arc(pos, r + 0.8, 0, TAU, 32, Color(1, 1, 0.4), 0.2)
		if near:
			art.draw_string(font, pos + Vector2(-r, r + 1.2), "g%d" % c.generation, HORIZONTAL_ALIGNMENT_LEFT, -1, 1, Color(1, 1, 1, 0.6))
	# colonies on top: overlapping membranes would hide the bridges
	_draw_colonies(cells, func(c): return cell_pos(c), 1.0)

## Colonies: members stuck together by adhesive products, joined by soft bridges.
func _draw_colonies(list: Array, pos_of: Callable, scale_r: float) -> void:
	var groups := {}
	for c in list:
		if c.colony >= 0:
			if not groups.has(c.colony):
				groups[c.colony] = []
			groups[c.colony].append(c)
	for k in groups:
		var g: Array = groups[k]
		if g.size() < 2:
			continue
		var centre := Vector2.ZERO
		for c in g:
			centre += pos_of.call(c)
		centre /= g.size()
		for c in g:
			var p: Vector2 = pos_of.call(c)
			art.draw_line(p, centre, Color(1.0, 0.85, 0.45, 0.9), 0.5 * scale_r)
			art.draw_circle(p, 0.45 * scale_r, Color(1.0, 0.85, 0.45, 0.9))
		art.draw_circle(centre, 0.8 * scale_r, Color(1.0, 0.85, 0.45, 0.95))

func hud_text() -> String:
	if not ok:
		return "[color=#f88]data/rates0.json missing: run primordium train[/color]"
	var s := stats
	if s.is_empty():
		return ""
	var colonial := 0
	for c in cells:
		if c.colony >= 0:
			colonial += 1
	var t := "[b]Beach[/b] sea left, dune right\ncells %d (surrogate %d)   generation %d   in colonies %d\ndivisions %d   deaths %d   genomes seen %d\nreplication error %.1f%%   radiation mutations %d\n" % [
		s.cells, s.surrogate, s.max_generation, colonial, s.divisions, s.deaths, s.genomes, s.copy_error * 100.0, s.radiation_mutations]
	t += "[b]Resolution[/b] %s   lifts %d\n" % ["auto (A)" if s.auto else "detail (A)", s.lifts]
	t += "[b]Discoveries[/b]\n"
	for d in s.discoveries:
		t += "[color=#cbf]" + d + "[/color]\n"
	if not detail.is_empty():
		t += "\n[b]Cell %d[/b]  gen %d  %s\nmembrane %d   monomers %d   genome %d letters\nradiation mutations %d   %s\n" % [
			detail.id, detail.generation, "surrogate" if detail.surrogate else "detail", detail.area, detail.free, detail.genome.size(), detail.mutations,
			"replicating %d/%d" % [detail.fork, detail.genome.size()] if detail.fork >= 0 else "not replicating"]
	t += "\n[color=#789]Tab: switch scale   A: auto resolution\nclick a cell, then a gene, then a residue[/color]"
	return t

## Genome map: bit ring (Li dark, Fl light), gene arcs by role, fork progress; gene list to click.
func _gene_rows() -> Array:
	var rows := []
	for k in detail.get("genes", []).size():
		rows.append(Rect2(176, 30 + k * 26, 146, 24))
	return rows

func _draw_genome() -> void:
	if detail.is_empty():
		return
	var font := ThemeDB.fallback_font
	genome_panel.draw_rect(Rect2(Vector2.ZERO, genome_panel.size), Color(0.03, 0.05, 0.08, 0.86))
	genome_panel.draw_string(font, Vector2(8, 16), "Genome of cell %d (binary: Li 0, Fl 1)" % detail.id, HORIZONTAL_ALIGNMENT_LEFT, -1, 12, Color(1, 1, 1, 0.8))
	var bits: PackedByteArray = detail.bits
	var n := bits.size()
	var c := Vector2(88, 130)
	var r := 70.0
	for i in n:
		var a := -PI / 2 + TAU * i / n
		var col := Color(0.92, 0.95, 1.0) if bits[i] == 1 else Color(0.25, 0.3, 0.4)
		genome_panel.draw_line(c + Vector2(cos(a), sin(a)) * (r - 5), c + Vector2(cos(a), sin(a)) * (r + 5), col, 2.0)
	var genes: Array = detail.genes
	for k in genes.size():
		var g: Dictionary = genes[k]
		var a0: float = -PI / 2 + TAU * g.start / n
		var a1: float = -PI / 2 + TAU * g.end / n
		var w := 5.0 if k == gene_sel else 3.0
		genome_panel.draw_arc(c, r - 12, a0, a1, 24, ROLE_COLORS[g.role], w)
	if detail.fork >= 0:
		genome_panel.draw_arc(c, r + 11, -PI / 2, -PI / 2 + TAU * detail.fork / max(n, 1), 48, Color(1, 0.9, 0.4, 0.9), 2.0)
	genome_panel.draw_string(font, c + Vector2(-26, 4), "%d letters" % n, HORIZONTAL_ALIGNMENT_LEFT, -1, 11, Color(1, 1, 1, 0.6))
	var rows := _gene_rows()
	for k in rows.size():
		var g: Dictionary = genes[k]
		var box: Rect2 = rows[k]
		genome_panel.draw_rect(box, Color(1, 1, 0.4, 0.2) if k == gene_sel else Color(1, 1, 1, 0.05))
		genome_panel.draw_rect(Rect2(box.position + Vector2(4, 8), Vector2(8, 8)), ROLE_COLORS[g.role])
		genome_panel.draw_string(font, box.position + Vector2(16, 16), "%s x%d" % [ROLE_NAMES[g.role], g.copies], HORIZONTAL_ALIGNMENT_LEFT, -1, 11, Color(1, 1, 1, 0.8))

func _genome_input(e: InputEvent) -> void:
	if not (e is InputEventMouseButton and e.pressed and e.button_index == MOUSE_BUTTON_LEFT):
		return
	var rows := _gene_rows()
	for k in rows.size():
		if rows[k].has_point(e.position):
			gene_sel = k
			inset.show_chain(detail.genes[k])
			inset.visible = true
			genome_panel.queue_redraw()
			genome_panel.accept_event()
			return

## Population means over time: evolution, plotted.
func _draw_chart() -> void:
	var font := ThemeDB.fallback_font
	var sz := chart.size
	chart.draw_rect(Rect2(Vector2.ZERO, sz), Color(0.03, 0.05, 0.08, 0.8))
	chart.draw_string(font, Vector2(8, 14), "Population means over time", HORIZONTAL_ALIGNMENT_LEFT, -1, 11, Color(1, 1, 1, 0.75))
	var rows := history.size() / 7
	if rows < 2:
		return
	var t0: float = history[0]
	var t1: float = history[(rows - 1) * 7]
	var span: float = max(t1 - t0, 0.001)
	var area := Rect2(8, 22, sz.x - 150, sz.y - 30)
	var series := [[1, 0.6, 0], [2, 0.6, 1], [3, 0.6, 2], [4, 1.0, 3]]
	for s in series:
		var pts := PackedVector2Array()
		for i in rows:
			var x: float = area.position.x + area.size.x * (history[i * 7] - t0) / span
			var v: float = history[i * 7 + s[0]] / s[1]
			pts.append(Vector2(x, area.position.y + area.size.y * (1.0 - clamp(v, 0.0, 1.0))))
		chart.draw_polyline(pts, ROLE_COLORS[s[2]], 1.5, true)
	var last := (rows - 1) * 7
	var lx: float = sz.x - 136
	for k in 4:
		var s: Array = series[k]
		chart.draw_rect(Rect2(lx, 22 + k * 22, 8, 8), ROLE_COLORS[s[2]])
		chart.draw_string(font, Vector2(lx + 12, 30 + k * 22), "%s %.2f" % [ROLE_NAMES[s[2]], history[last + s[0]]], HORIZONTAL_ALIGNMENT_LEFT, -1, 10, Color(1, 1, 1, 0.75))
	chart.draw_string(font, Vector2(8, sz.y - 4), "day %.0f .. %.0f" % [t0, t1], HORIZONTAL_ALIGNMENT_LEFT, -1, 10, Color(1, 1, 1, 0.5))

func screen_to_world(sp: Vector2) -> Vector2:
	var vs := get_viewport_rect().size
	return (cam.position + (sp - vs * 0.5) / cam.zoom) / CELL

func zoom_at(factor: float, sp: Vector2) -> void:
	var vs := get_viewport_rect().size
	var before := cam.position + (sp - vs * 0.5) / cam.zoom
	var z: float = clamp(cam.zoom.x * factor, 0.3, 20.0)
	cam.zoom = Vector2(z, z)
	cam.position = before - (sp - vs * 0.5) / cam.zoom

func select_at(p: Vector2) -> void:
	selected = -1
	var best := 1e9
	for c in cells:
		var d := cell_pos(c).distance_to(p)
		if d <= c.r + 0.5 and d < best:
			best = d
			selected = c.id
	detail = sim.cell_detail(selected) if selected >= 0 else {}
	gene_sel = -1
	inset.show_chain({})
	inset.visible = false
	genome_panel.visible = not detail.is_empty()
	genome_panel.queue_redraw()

## Capture helper: the cell with the most genes, its first gene, a residue.
func pick_longest() -> void:
	var list: Array = cells if not cells.is_empty() else sim.cells()
	var best := -1
	var most := -1
	for c in list:
		if c.genes > most:
			most = c.genes
			best = c.id
	selected = best
	detail = sim.cell_detail(best) if best >= 0 else {}
	if not detail.is_empty() and detail.genes.size() > 0:
		gene_sel = 0
		inset.show_chain(detail.genes[0])
		inset.residue = 1
		inset.visible = true
	genome_panel.visible = not detail.is_empty()

func _unhandled_input(e: InputEvent) -> void:
	if e is InputEventMouseButton:
		if e.button_index == MOUSE_BUTTON_WHEEL_UP and e.pressed:
			zoom_at(1.15, e.position)
		elif e.button_index == MOUSE_BUTTON_WHEEL_DOWN and e.pressed:
			zoom_at(1.0 / 1.15, e.position)
		elif e.button_index in [MOUSE_BUTTON_RIGHT, MOUSE_BUTTON_MIDDLE]:
			dragging = e.pressed
		elif e.button_index == MOUSE_BUTTON_LEFT and e.pressed:
			select_at(screen_to_world(e.position))
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
