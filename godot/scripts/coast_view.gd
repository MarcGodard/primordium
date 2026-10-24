## Scale 4 view: a coastline of beaches, north at the top. Each strip is one site (sea left,
## dune right) at its own latitude: warmer, longer days near the equator, bigger tides in the
## bays at both ends. Cells drift between neighbours on the current at high water.
## Cell tint = what its genes do, as on scale 3. Bottom chart: each site's mean tint over time.
extends Node2D
var campaign: Node

const CELL := 6.0
const SPEEDS := [0, 1, 2, 4, 8]
const DT := 0.02
const EXPLAIN := """[b]Scale 4: a coastline on an Earth-sized planet[/b]

[b]What you see.[/b] Twelve beaches along one coast, from far south (bottom) to far north (top), on a planet the size of Earth: neighbours are about 1,270 km apart. The globe shows where they are, lit by the sun at this hour and season. Each strip is a whole beach of cells; the bars show its average genes.

[b]Climate makes niches.[/b] Near the equator it is warm with long even days; near the poles the seasons swing hard. Tides are biggest in the bays at both ends. Ocean currents carry loose cells from beach to beach, so a good gene can spread along the coast.

[b]Learned models.[/b] A whole coast of chemistry is slow, so a beach can switch to [b]lineage mode[/b] (purple frame): instead of simulating every cell's chemistry, cells divide and die at rates learned from the detailed beaches. The model is checked against the real thing (about 97% as close as two real runs are to each other).

[b]Real world.[/b] Populations adapt to local climate: the bottom chart shows which trait each beach leads in, over time. Here, copying enzymes grew stronger at the warm equator and weaker at the cold poles, where cold alone keeps copying accurate.

[b]Try.[/b] Click a beach (or a dot on the globe) and zoom in to see its real cells."""
const SCALE_NAME := "scale 4: coastline"
const BAND := 22.0
const TILE_W := 220.0
const TILE_H := 44.0
const GAP := 8.0
const COL_GAP := 60.0
var cols := 1
const ROLE_COLORS := [Color(0.3, 0.85, 0.85), Color(0.45, 0.85, 0.4), Color(0.95, 0.9, 0.75), Color(0.95, 0.65, 0.2), Color(0.6, 0.6, 0.65)]
const ROLE_NAMES := ["polymerase", "metabolism", "membrane", "pigment"]

## Environment levers (L): real rates in the model, x1/4..x4 (lever_panel.gd).
const LEVERS := [["current", "Ocean current", "How fast the along-shore current carries flooded cells to the next beach. Faster: more travellers and beaches mix; slower: each beach evolves on its own."]]
var levers: PanelContainer
var levers_open := false
var sim: CoastSim
var world: Node2D
var art: Node2D
var cam: Camera2D
var tiles: Array = []
var speed_idx := 2
var env := {}
var stats := {}
var sites: Array = []
var site_cells: Array = []
var selected := -1
var history := PackedFloat32Array()
var chart: Control
var globe: SubViewportContainer
var dragging := false
var timer := 0.0
var ok := false
var auto := true
var n_sites := 0

func _ready() -> void:
	sim = CoastSim.new()
	add_child(sim)
	ok = sim.start(ProjectSettings.globalize_path("res://../" + ElementsSim.data_dir() + "rates0.json"), 1)
	if campaign:
		campaign.graduated.connect(_on_graduated)
	n_sites = sim.site_count()
	# wide screens (ultrawide) take the coast in two columns, north half on the left
	var vs0 := get_viewport_rect().size
	cols = 2 if vs0.x / vs0.y > 2.0 and n_sites > 1 else 1
	world = Node2D.new()
	world.scale = Vector2(CELL, CELL)
	add_child(world)
	for k in n_sites:
		var r := ColorRect.new()
		r.size = Vector2(TILE_W, TILE_H)
		r.position = tile_origin(k)
		var m := ShaderMaterial.new()
		m.shader = load("res://shaders/beach.gdshader")
		m.set_shader_parameter("grid", Vector2(TILE_W, TILE_H))
		m.set_shader_parameter("elev_range", Vector2(0.28, 0.77))
		r.material = m
		r.mouse_filter = Control.MOUSE_FILTER_IGNORE
		world.add_child(r)
		tiles.append(r)
	art = Node2D.new()
	art.draw.connect(_draw_coast)
	world.add_child(art)
	cam = Camera2D.new()
	add_child(cam)
	var total_h := per_col() * (TILE_H + GAP)
	var total_w := cols * (TILE_W + 140.0) + (cols - 1) * COL_GAP
	var vs := get_viewport_rect().size
	var fit: float = min(vs.x * 0.6 / (total_w * CELL), (vs.y - 255.0) / (total_h * CELL)) * 0.95
	cam.zoom = Vector2(fit, fit)
	# strips between the goal banner (top ~105 px) and the evolution chart (bottom ~150 px)
	cam.position = Vector2(total_w * 0.5 - 50.0, total_h * 0.5) * CELL + Vector2(0, 22.0 / fit)
	var layer := CanvasLayer.new()
	layer.layer = 2
	add_child(layer)
	levers = load("res://scripts/lever_panel.gd").new()
	levers.setup("Environment levers (L to hide)", LEVERS, func(k: String, m: float) -> bool: return sim.set_lever(k, m), func() -> Dictionary: return sim.levers(), "Run an experiment: change it, let time run, watch migrations and how alike the beaches get.")
	levers.position = Vector2(12, 440)
	layer.add_child(levers)
	chart = Control.new()
	chart.anchor_left = 0.5
	chart.anchor_right = 0.5
	chart.anchor_top = 1.0
	chart.anchor_bottom = 1.0
	chart.offset_left = -330
	chart.offset_right = 330
	chart.offset_top = -142
	chart.offset_bottom = -12
	chart.mouse_filter = Control.MOUSE_FILTER_IGNORE
	chart.draw.connect(_draw_chart)
	layer.add_child(chart)
	globe = load("res://scripts/globe.gd").new()
	globe.position = Vector2(12, 600)
	globe.size = Vector2(330, 330)
	globe.site_clicked.connect(func(k: int) -> void: selected = k)
	layer.add_child(globe)
	if ok:
		sim.step(1, DT)

## Sites per column.
func per_col() -> int:
	return int(ceil(float(n_sites) / cols))

## Column of site k: northern sites first (left), so reading order runs north to south.
func col_of(k: int) -> int:
	return (n_sites - 1 - k) / per_col()

## Site k's strip; site 0 is the southernmost, drawn last (bottom of the last column).
func tile_origin(k: int) -> Vector2:
	var i := n_sites - 1 - k
	return Vector2(col_of(k) * (TILE_W + 140.0 + COL_GAP), (i % per_col()) * (TILE_H + GAP))

func activate(on: bool) -> void:
	visible = on
	set_process(on)
	set_process_unhandled_input(on)
	levers.visible = on and levers_open
	chart.visible = on
	globe.visible = on
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

func focus_site() -> int:
	return selected if selected >= 0 else n_sites / 2

func _process(delta: float) -> void:
	if not ok:
		return
	if speed() > 0:
		sim.step(speed(), DT)
	env = sim.env(focus_site())
	sites = sim.sites()
	for k in sites.size():
		var s: Dictionary = sites[k]
		var m: ShaderMaterial = tiles[k].material
		m.set_shader_parameter("tide_level", s.tide_level)
		m.set_shader_parameter("light", clamp(s.visible * 1.6 + 0.12, 0.0, 1.0))
	timer -= delta
	if timer <= 0.0:
		timer = 0.4
		stats = sim.stats()
		history = sim.history()
		chart.queue_redraw()
	if selected >= 0 and px_per_unit() > 2.0:
		site_cells = sim.site_cells(selected)
	art.queue_redraw()
	# beads take the evolution chart's colours: the trait each site leads in vs the coast
	var tints := []
	var mean := [0.0, 0.0, 0.0, 0.0]
	for s in sites:
		var v := [s.poly, s.synth, s.amph, s.pigment]
		for r in 4:
			mean[r] += v[r] / max(sites.size(), 1)
	for s in sites:
		var v := [s.poly, s.synth, s.amph, s.pigment]
		var rel := []
		for r in 4:
			rel.append(max(0.0, 1.0 + 6.0 * (v[r] - mean[r]) / max(mean[r], 0.01)) * [0.5, 0.5, 0.4, 0.8][r])
		tints.append(tint(rel[0], rel[1], rel[2], rel[3]))
	globe.refresh(sim.env(n_sites / 2), sites, tints, selected)

func tint(poly: float, synth: float, amph: float, pigment: float) -> Color:
	var w := [poly / 0.5, synth / 0.5, amph / 0.4, pigment / 0.8]
	var col := Color(0, 0, 0, 0)
	var tot := 0.0
	for k in 4:
		col += ROLE_COLORS[k] * w[k]
		tot += w[k]
	return (col / max(tot, 0.001)) if tot > 0.01 else ROLE_COLORS[4]

## Warm to cold: the site's temperature as a label colour.
func temp_color(t: float) -> Color:
	return Color(0.45, 0.65, 1.0).lerp(Color(1.0, 0.55, 0.35), clamp((t - 270.0) / 50.0, 0.0, 1.0))

func _draw_coast() -> void:
	var font := ThemeDB.fallback_font
	var tsec := Time.get_ticks_msec() / 1000.0
	for k in sites.size():
		var s: Dictionary = sites[k]
		var o := tile_origin(k)
		# labels left of the strip: latitude and temperature
		art.draw_string(font, o + Vector2(-40, 14), "%+.0f°" % rad_to_deg(s.latitude), HORIZONTAL_ALIGNMENT_LEFT, -1, 12, Color(1, 1, 1, 0.85))
		art.draw_string(font, o + Vector2(-40, 26), "%.0f K" % s.temperature_k, HORIZONTAL_ALIGNMENT_LEFT, -1, 11, temp_color(s.temperature_k))
		art.draw_string(font, o + Vector2(-40, 37), "tide x%.1f" % s.tide_scale, HORIZONTAL_ALIGNMENT_LEFT, -1, 9, Color(0.6, 0.8, 1.0, 0.8))
		# population as dots; the selected site shows real cells instead
		if k != selected or site_cells.is_empty():
			var col := tint(s.poly, s.synth, s.amph, s.pigment)
			var n: int = s.cells
			for i in n:
				var h := fmod(sin(float(i) * 12.9898 + k * 78.233) * 43758.5453, 1.0)
				var v := fmod(sin(float(i) * 39.3468 + k * 11.135) * 24634.6345, 1.0)
				h = h if h >= 0.0 else h + 1.0
				v = v if v >= 0.0 else v + 1.0
				var p := o + Vector2(TILE_W * (0.05 + 0.9 * h), TILE_H * (0.08 + 0.84 * v))
				art.draw_circle(p, 0.9, Color(col, 0.75))
		# phenotype bars right of the strip
		var vals := [s.poly / 0.5, s.synth / 0.5, s.amph / 0.4, s.pigment / 0.8]
		for r in 4:
			var bw: float = 40.0 * clamp(vals[r], 0.0, 1.5) / 1.5
			art.draw_rect(Rect2(o + Vector2(TILE_W + 6, 4 + r * 9), Vector2(bw, 6)), ROLE_COLORS[r])
		# what this site's genes listen to: always on, light, cold, food short, mates, height, skin (stacked)
		var reg: PackedFloat32Array = s.get("reg", PackedFloat32Array())
		if reg.size() >= 7:
			var x0 := TILE_W + 6.0
			var cols := [Color(0.55, 0.55, 0.6), Color(1.0, 0.9, 0.35), Color(0.45, 0.75, 1.0), Color(0.85, 0.45, 0.9), Color(1.0, 0.6, 0.35), Color(0.4, 0.9, 0.55), Color(0.95, 0.5, 0.6)]
			for r in 7:
				var w: float = 40.0 * reg[r]
				art.draw_rect(Rect2(o + Vector2(x0, 39), Vector2(w, 4)), cols[r])
				x0 += w
		art.draw_string(font, o + Vector2(TILE_W + 50, 13), "%d cells" % s.cells, HORIZONTAL_ALIGNMENT_LEFT, -1, 10, Color(1, 1, 1, 0.8))
		art.draw_string(font, o + Vector2(TILE_W + 50, 25), "gen %d" % s.max_generation, HORIZONTAL_ALIGNMENT_LEFT, -1, 10, Color(1, 1, 1, 0.65))
		art.draw_string(font, o + Vector2(TILE_W + 50, 37), "err %.1f%%" % (s.bit_error * 100.0), HORIZONTAL_ALIGNMENT_LEFT, -1, 10, Color(1, 1, 1, 0.65))
		if s.lineage:
			# lineage mode: cells divide at learned rates instead of running their chemistry
			art.draw_rect(Rect2(o, Vector2(TILE_W, TILE_H)), Color(0.75, 0.6, 1.0, 0.7), false, 0.4)
			art.draw_string(font, o + Vector2(TILE_W - 34, 8), "lineage", HORIZONTAL_ALIGNMENT_LEFT, -1, 7, Color(0.8, 0.7, 1.0, 0.9))
		if k == selected:
			art.draw_rect(Rect2(o - Vector2(1, 1), Vector2(TILE_W + 2, TILE_H + 2)), Color(1, 1, 0.4, 0.9), false, 0.6)
		# current between neighbours: drifting dashes in the gap
		if k + 1 < sites.size() and col_of(k + 1) == col_of(k):
			var gy := o.y - GAP * 0.5
			for d in 12:
				var x := fmod(d * 20.0 + tsec * 6.0 * (1.0 if k % 2 == 0 else -1.0), TILE_W)
				x = x if x >= 0.0 else x + TILE_W
				art.draw_line(Vector2(o.x + x, gy), Vector2(o.x + x + 4.0, gy), Color(0.5, 0.75, 1.0, 0.3), 0.4)
	# zoomed into the selected site: real cells, placed as on scale 3
	if selected >= 0:
		var o := tile_origin(selected)
		var sy := TILE_H / 140.0
		var place := func(c: Dictionary) -> Vector2:
			var jit: float = fmod(float(c.id) * 0.618, 1.0)
			return o + Vector2((float(c.patch) + 0.15 + 0.7 * jit) * BAND, float(c.y) * 140.0 * sy)
		for c in site_cells:
			var jitter: float = fmod(float(c.id) * 0.618, 1.0)
			var p := o + Vector2((float(c.patch) + 0.15 + 0.7 * jitter) * BAND, float(c.y) * 140.0 * sy)
			var col := tint(c.poly, c.synth, c.amph, c.pigment)
			var r: float = c.r * sy * 1.6
			art.draw_circle(p, r, Color(col, 0.35))
			art.draw_arc(p, r, 0, TAU, 16, Color(0.75, 0.6, 1.0, 0.9) if c.surrogate else Color(col.lightened(0.3), 0.9), 0.12)
		# bridges on top: translucent cells would paint over them
		_draw_colonies(site_cells, place)

## Colonies: members stuck together by adhesive products, joined by soft bridges.
func _draw_colonies(list: Array, pos_of: Callable) -> void:
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
		# positions wrap along the shore: take each member the short way round from the first
		var first: Vector2 = pos_of.call(g[0])
		var pts := []
		for c in g:
			var q: Vector2 = pos_of.call(c)
			if q.y - first.y > TILE_H * 0.5:
				q.y -= TILE_H
			elif first.y - q.y > TILE_H * 0.5:
				q.y += TILE_H
			pts.append(q)
		var centre := Vector2.ZERO
		for q in pts:
			centre += q
		centre /= g.size()
		for q in pts:
			art.draw_line(q, centre, Color(0.95, 0.8, 0.5, 0.55), 0.12)
		art.draw_circle(centre, 0.25, Color(0.95, 0.8, 0.5, 0.8))

## Campaign goal: cells carried by the current to another beach.
const GOAL_MIGRANTS := 300
func goal_progress() -> Dictionary:
	var v: int = stats.get("migrations", 0)
	return {"value": v, "p": clamp(float(v) / GOAL_MIGRANTS, 0.0, 1.0), "done": v >= GOAL_MIGRANTS, "args": [GOAL_MIGRANTS], "now": "%d cells have crossed so far" % v}

func hud_text() -> String:
	if not ok:
		return "[color=#f88]%srates0.json missing: run primordium train[/color]" % ElementsSim.data_dir()
	if stats.is_empty():
		return ""
	var t := "[b]Coast[/b] %d sites, north at top\n" % n_sites + "[color=#9ab]Earth-sized planet (radius %.0f km): sites %.0f km apart, current %.1f m/s[/color]\n" % [stats.radius_km, stats.spacing_km, stats.current_m_s] + "cells %d   migrations %d\n[b]Resolution[/b] %s   lineage sites %d   lifts %d\n" % [
		stats.cells, stats.migrations, "auto (A)" if stats.auto else "detail (A)", stats.lineage_sites, stats.lifts]
	t += "[color=#cbf]purple frame: site runs as lineages (learned division rates)[/color]\n"
	t += "[color=#9ab]thin bar under traits: what genes listen to: grey always on, [color=#ffe65a]light[/color], [color=#73bfff]cold[/color], [color=#d973e6]food short[/color], [color=#ff9959]mates[/color][/color]\n"
	if selected >= 0 and selected < sites.size():
		var s: Dictionary = sites[selected]
		t += "\n[b]Site %+.0f°[/b]  %.1f K  tide x%.1f\ncells %d   generation %d   divisions %d\nbit error %.2f%%   genome %.0f letters\nin colonies %.0f%% (mean size %.1f)   adhesion %.2f\nengulfed %d\npolymerase %.2f  metabolism %.2f\nmembrane %.2f  pigment %.2f\n" % [
			rad_to_deg(s.latitude), s.temperature_k, s.tide_scale, s.cells, s.max_generation, s.divisions, s.bit_error * 100.0, s.genome_len, s.colonial * 100.0, s.colony_size, s.adhesion, s.eaten, s.poly, s.synth, s.amph, s.pigment]
		t += "[color=#9ab]sky and tide shown for this site[/color]\n"
	t += "[b]Discoveries[/b]\n"
	for d in stats.discoveries:
		t += "[color=#cbf]" + d + "[/color]\n"
	if campaign and campaign.state.has("coast_founders"):
		t += "[color=#ffd080]%d of your coast's cells are the reef's founders (U: send these instead)[/color]\n" % campaign.state.coast_founders.size()
	t += "\n[color=#789]Tab: switch scale   A: auto resolution   L: levers\nU: send this coast's survivors up to the reef\nclick a site, zoom in to see its cells[/color]"
	return t

## Evolution map: one row per site (north on top), one column per day, colour = mean tint.
func _draw_chart() -> void:
	var font := ThemeDB.fallback_font
	var sz := chart.size
	chart.draw_rect(Rect2(Vector2.ZERO, sz), Color(0.03, 0.05, 0.08, 0.82))
	chart.draw_string(font, Vector2(8, 14), "Evolution along the coast (row = site, north on top; colour = trait a site leads in vs the coast)", HORIZONTAL_ALIGNMENT_LEFT, -1, 11, Color(1, 1, 1, 0.75))
	var stride := 1 + n_sites * 5
	var rows := history.size() / stride
	if rows < 2 or n_sites == 0:
		return
	var area := Rect2(8, 22, sz.x - 16, sz.y - 40)
	var cw: float = area.size.x / rows
	var ch: float = area.size.y / n_sites
	for i in rows:
		# each trait relative to the coast mean that day: shows which sites lead in what
		var mean := [0.0, 0.0, 0.0, 0.0]
		for k in n_sites:
			for r in 4:
				mean[r] += history[i * stride + 1 + k * 5 + r] / n_sites
		for k in n_sites:
			var b := i * stride + 1 + k * 5
			var rel := []
			for r in 4:
				rel.append(max(0.0, 1.0 + 6.0 * (history[b + r] - mean[r]) / max(mean[r], 0.01)) * [0.5, 0.5, 0.4, 0.8][r])
			var col := tint(rel[0], rel[1], rel[2], rel[3])
			var fill: float = clamp(history[b + 4] / 200.0, 0.15, 1.0)
			chart.draw_rect(Rect2(area.position.x + i * cw, area.position.y + (n_sites - 1 - k) * ch, cw + 0.5, ch - 1.0), Color(col * fill, 1.0))
	chart.draw_string(font, Vector2(8, sz.y - 5), "day %.0f .. %.0f" % [history[0], history[(rows - 1) * stride]], HORIZONTAL_ALIGNMENT_LEFT, -1, 10, Color(1, 1, 1, 0.5))
	for r in 4:
		chart.draw_rect(Rect2(sz.x - 330 + r * 82, sz.y - 13, 8, 8), ROLE_COLORS[r])
		chart.draw_string(font, Vector2(sz.x - 318 + r * 82, sz.y - 5), ROLE_NAMES[r], HORIZONTAL_ALIGNMENT_LEFT, -1, 10, Color(1, 1, 1, 0.6))

func screen_to_world(sp: Vector2) -> Vector2:
	var vs := get_viewport_rect().size
	return (cam.position + (sp - vs * 0.5) / cam.zoom) / CELL

func zoom_at(factor: float, sp: Vector2) -> void:
	var vs := get_viewport_rect().size
	var before := cam.position + (sp - vs * 0.5) / cam.zoom
	var z: float = clamp(cam.zoom.x * factor, 0.2, 20.0)
	cam.zoom = Vector2(z, z)
	cam.position = before - (sp - vs * 0.5) / cam.zoom

func select_at(p: Vector2) -> void:
	selected = -1
	site_cells = []
	for k in n_sites:
		if Rect2(tile_origin(k) - Vector2(42, 0), Vector2(TILE_W + 140, TILE_H)).has_point(p):
			selected = k
			site_cells = sim.site_cells(k)

## Capture helper: select the middle site.
func pick_longest() -> void:
	selected = n_sites / 2
	site_cells = sim.site_cells(selected)

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
			KEY_U:
				if not e.echo:
					hand_up()
			KEY_L:
				if not e.echo:
					levers_open = not levers_open
					levers.visible = levers_open
					levers.refresh()
			KEY_SPACE:
				speed_idx = 0 if speed_idx != 0 else 2
			KEY_1, KEY_2, KEY_3, KEY_4, KEY_5:
				speed_idx = e.keycode - KEY_1
			KEY_A:
				auto = not auto
				sim.set_auto(auto)

## The recipe changed: every beach starts again with the new sea.
func rebuild_world() -> bool:
	selected = -1
	ok = sim.start(ProjectSettings.globalize_path("res://../" + ElementsSim.data_dir() + "rates0.json"), 1)
	levers.refresh()
	return true

## When the coast's goal is met (this session, on this coast), its survivors go up once on
## their own: never on a later visit, when the coast has just restarted from day 0.
func _on_graduated(k: int, _page: String) -> void:
	if k == 5 and active_here():
		hand_up()

func active_here() -> bool:
	return is_visible_in_tree() and ok

## Send this coast's living cells up: they found every reef, archipelago and your world from
## now on (worlds already built rebuild when next visited). The shipped founders are only the
## fallback before any coast was handed up.
func hand_up() -> void:
	if not ok or campaign == null:
		return
	var f: Array = sim.founders(96)
	campaign.set_founders(f)
	if f.is_empty():
		campaign.toast.emit("No survivors to send", "This coast has no living cells: the reef above starts empty. Tune the coast and send again.", Color(1.0, 0.55, 0.45))
	else:
		campaign.toast.emit("Founders sent up", "%d of your coast's cells now found the reef and the islands. Worlds above rebuild from them when you next visit." % f.size(), Color(1.0, 0.8, 0.45))
