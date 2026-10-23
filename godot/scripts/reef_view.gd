## Scale 5 view: the reef in 3D. Sea in front, the shore rising away to the dune. Every sphere
## is a cell; a body is a clone stuck together. Tint = what the cell's genes are doing where it
## sits: teal polymerase, green metabolism, cream membrane, crimson pigment, gold glue.
## Sun, moons, clouds, tide and swell all follow the cosmos. Spores and torn pieces drift glowing
## on the surface until they settle.
extends Node3D

const SPEEDS := [0, 1, 5, 20, 80]
## Days per second per speed unit.
const DAYS_PER_UNIT := 0.05
const DT := 0.02
const EXPLAIN := """[b]Scale 5: a reef of many-celled bodies[/b]

[b]What you see.[/b] A rocky shore in 3D, from deep water up to dry sand. Every sphere is a cell; cells stuck together are one [b]body[/b], all with the same genome (a clone, like a plant or coral). The sea rises and falls with the tide; sun, moons, clouds and seasons come from the same sky model as every other scale.

[b]A body is a team.[/b] All its cells carry the same genes, but each switches them on by where it sits: how much light reaches it, how high it is, whether it touches the water. Tint shows what a cell does more than average ([color=#40ccd9]copying[/color], [color=#59cc4d]metabolism[/color], [color=#f2e0b8]membrane[/color], [color=#bf3852]pigment[/color], [color=#ffbf33]glue[/color]). That is [b]cell differentiation[/b], like leaf and root cells in one plant.

[b]Life on the reef.[/b] Food comes in through the cells touching water, and flows inward. Waves tear tall bodies (glue at the base holds them). Pigment on top shades the cells below from UV. Sticky bodies graze on smaller ones (red flash). A daughter cell that does not stick floats away as a [b]spore[/b] (glowing on the surface) and starts a new body, with a few copy mistakes in its genome: that is how bodies evolve. Torn-off pieces drift and regrow too.

[b]Evolution you can see.[/b] Press C to colour bodies by [b]lineage[/b] (which founder they descend from). Over a few hundred days most lineages die out and the survivors split the shore: tall bodies in calm deep water, low crusts where waves break.

[b]Try.[/b] Click a body to see its layers and genes, B for the biggest, right-drag to orbit, dive under the surface."""
var SCALE_NAME := "scale 5: reef"
const GARDEN_EXPLAIN := """[b]Your world[/b]

[b]What this is.[/b] The same kind of reef as scale 5, but it starts empty. Every body you keep at the reef (select it, press K) arrives here as a handful of spores, as a lineage of its own, named.

[b]What happens.[/b] Your creatures share one shore, one ocean of food and one sky. They grow, shed spores, mutate and compete. Lineages that fit spread, the others fade. What you see here a few hundred days later is no longer what you kept: it kept evolving.

[b]It lasts.[/b] When you leave, the surviving lineages are saved and come back next time. Keep creatures from several runs of the reef (N starts a new run) and watch which ones win.

[b]Run experiments.[/b] L opens the environment levers: waves, grazers, food and UV, each a real rate in the model (x1/4 to x4). Change one, fast-forward (T) and read the report. In our tests the waves mattered most: in calm water big bodies keep their division of labour, in storms they are torn apart before it pays. Is that what you find too?

[b]Fast-forward.[/b] T runs 100 days (Shift+T 500) and tells you what evolution did. While the game is closed your world keeps running.

[b]Try.[/b] C colours bodies by lineage. Click a body to see whose descendant it is, K to keep it again."""
const ROLE_COLORS := [Color(0.25, 0.8, 0.85), Color(0.35, 0.8, 0.3), Color(0.95, 0.88, 0.72), Color(0.75, 0.22, 0.32), Color(0.6, 0.6, 0.65), Color(1.0, 0.75, 0.2)]
const ROLE_NAMES := ["polymerase", "metabolism", "membrane", "pigment", "no clear role", "glue"]

var sim: ReefSim
var ok := false
var cam: Camera3D
var rig: Node3D
var water: MeshInstance3D
var water_mat: ShaderMaterial
var rock_mat: ShaderMaterial
var cells_mm: MultiMesh
var cell_mat: ShaderMaterial
var bites_mm: MultiMesh
var rings_mm: MultiMesh
## Smooth skins (BodySkin) over the selected body and the creatures, id -> MeshInstance3D.
var skins := {}
var skin_mat: StandardMaterial3D
## S: skins on every body of SKIN_MIN+ cells, not only the selected one and the creatures.
var skin_all := false
const SKIN_MIN := 4
## Bodies offered per half-second tick for a rebuild; the rest keep their last mesh.
const SKIN_BATCH := 40
## Time a tick may spend rebuilding skins (a 400-cell body takes ~4 ms plus upload).
const SKIN_BUDGET_US := 6000
var skin_turn := 0
const Gallery := preload("res://scripts/gallery.gd")
var creatures := PackedInt64Array()
var creature_at := 0
var speed_idx := 2
var env := {}
var stats := {}
var organism := {}
var history := PackedFloat32Array()
var selected := -1
var timer := 0.0
var acc := 0.0
var extent := Vector2.ZERO
# orbit camera
var target := Vector3.ZERO
var yaw := 0.0
var pitch := -0.38
var dist := 120.0
var orbiting := false
var panning := false
var auto_orbit := true
var layer: CanvasLayer
var profile: Control
var chart: Control
var active := false
var by_lineage := false
## Garden: the player's own reef, no founders; kept creatures are released into it.
var garden := false
var campaign: Node
## Garden lineage number -> creature name.
var names := {}
## Garden lineages that are the player's creatures (kept, not wild offshoots): always saved.
var mine := {}
## Saved lineages that could not be released this session: carried over to the next save.
var unplaced := []
var save_timer := 0.0
var restored := false
const RATES := "res://../data/rates0.json"
const SEED := "res://../data/reef_seed.json"
## Spores per release: one alone usually dies young.
const RELEASE_SPORES := 12
## Fast-forward: T runs this many days on a worker thread, Shift+T five times as many.
const FF_DAYS := 100.0
## Your world keeps evolving while the game is closed: days per real hour away, capped.
const AWAY_DAYS_PER_HOUR := 10.0
const AWAY_MAX_DAYS := 300.0
const TRAIT_NAMES := ["copying (polymerase)", "metabolism", "membrane", "pigment", "glue"]
## What a rise in each trait means on the reef, for the report.
const TRAIT_WHY := [
	"more copying machinery: faster, more faithful genome copies",
	"more metabolism: cells make more of their own building blocks",
	"more membrane: on the skin it beats like cilia, so bodies crawl more",
	"more pigment: shade against UV, for the cell and the ones below it",
	"more glue: a firmer grip against waves, and grazers bite with it",
]
var ff: CanvasLayer
var ff_title := ""
## Your world's levers (L): multipliers on real reef rates, see ReefSim.set_lever.
const LEVERS := [
	["waves", "Waves", "How hard the surf pulls on bodies. Calm water lets tall, many-celled bodies keep their shape; storms tear them into pieces."],
	["grazers", "Grazers", "How often sticky bodies bite smaller ones. Predation pressure."],
	["food", "Food", "How fast cells take food from the water through their open faces."],
	["uv", "UV", "How deadly unshaded sunlight is. Pigment protects."],
]
var lever_panel: PanelContainer
var wave_mult := 1.0
## Unnamed lineages a garden save keeps (biggest first); named ones are always kept.
const GARDEN_KEEP := 32

func _ready() -> void:
	sim = ReefSim.new()
	add_child(sim)
	if garden:
		SCALE_NAME = "your world"
		ok = sim.start_garden(ProjectSettings.globalize_path(RATES), ProjectSettings.globalize_path(SEED), 7)
	else:
		ok = sim.start(ProjectSettings.globalize_path(RATES), ProjectSettings.globalize_path(SEED), campaign.state.run if campaign else 1)
	extent = sim.extent()
	target = Vector3(extent.x * 0.5, 30.0, -extent.y * 0.35)
	_build_environment()
	_build_terrain()
	_build_water()
	_build_cells()
	cam = Camera3D.new()
	cam.fov = 50.0
	cam.near = 0.3
	cam.far = 4000.0
	add_child(cam)
	_build_overlay()
	ff = load("res://scripts/ff_panel.gd").new()
	add_child(ff)
	if garden:
		_build_levers()
	if ok:
		sim.step(1, DT)

func _build_environment() -> void:
	rig = load("res://scripts/sky_rig.gd").new()
	add_child(rig)

## Rock from the sim's height grid; a seafloor apron runs out under the open sea.
func _build_terrain() -> void:
	var h: PackedFloat32Array = sim.heights()
	var w := int(extent.x)
	var d := int(extent.y)
	if h.size() != (w + 1) * (d + 1):
		return
	var verts := PackedVector3Array()
	var norms := PackedVector3Array()
	var idx := PackedInt32Array()
	verts.resize((w + 1) * (d + 1))
	norms.resize(verts.size())
	for y in d + 1:
		for x in w + 1:
			var i := y * (w + 1) + x
			verts[i] = Vector3(x, h[i], -y)
			var hl: float = h[y * (w + 1) + max(x - 1, 0)]
			var hr: float = h[y * (w + 1) + min(x + 1, w)]
			var hd: float = h[max(y - 1, 0) * (w + 1) + x]
			var hu: float = h[min(y + 1, d) * (w + 1) + x]
			norms[i] = Vector3(hl - hr, 2.0, hu - hd).normalized()
	for y in d:
		for x in w:
			var a := y * (w + 1) + x
			var b := a + 1
			var c := a + w + 1
			var e := c + 1
			idx.append_array([a, c, b, b, c, e])
	var arrays := []
	arrays.resize(Mesh.ARRAY_MAX)
	arrays[Mesh.ARRAY_VERTEX] = verts
	arrays[Mesh.ARRAY_NORMAL] = norms
	arrays[Mesh.ARRAY_INDEX] = idx
	var mesh := ArrayMesh.new()
	mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)
	rock_mat = ShaderMaterial.new()
	rock_mat.shader = load("res://shaders/rock.gdshader")
	var mi := MeshInstance3D.new()
	mi.mesh = mesh
	mi.material_override = rock_mat
	add_child(mi)
	# seafloor beyond the reef's deep edge and along its sides
	var floor_y: float = h[0] - 3.0
	var apron := PlaneMesh.new()
	apron.size = Vector2(3000, 3000)
	var ai := MeshInstance3D.new()
	ai.mesh = apron
	ai.position = Vector3(extent.x * 0.5, floor_y, -extent.y * 0.5)
	ai.material_override = rock_mat
	add_child(ai)

func _build_water() -> void:
	var pm := PlaneMesh.new()
	pm.size = Vector2(1400, 1400)
	pm.subdivide_width = 160
	pm.subdivide_depth = 160
	water_mat = ShaderMaterial.new()
	water_mat.shader = load("res://shaders/water.gdshader")
	water = MeshInstance3D.new()
	water.mesh = pm
	water.material_override = water_mat
	water.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	water.position = Vector3(extent.x * 0.5, 0.0, -extent.y * 0.5)
	add_child(water)

func _build_cells() -> void:
	var sphere := SphereMesh.new()
	sphere.radius = 1.0
	sphere.height = 2.0
	sphere.radial_segments = 14
	sphere.rings = 8
	cell_mat = ShaderMaterial.new()
	cell_mat.shader = load("res://shaders/cell3d.gdshader")
	cells_mm = MultiMesh.new()
	cells_mm.transform_format = MultiMesh.TRANSFORM_3D
	cells_mm.use_colors = true
	cells_mm.use_custom_data = true
	cells_mm.mesh = sphere
	var mi := MultiMeshInstance3D.new()
	mi.multimesh = cells_mm
	mi.material_override = cell_mat
	add_child(mi)
	var spark := SphereMesh.new()
	spark.radius = 1.0
	spark.height = 2.0
	spark.radial_segments = 8
	spark.rings = 4
	var sm := StandardMaterial3D.new()
	sm.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	sm.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	sm.vertex_color_use_as_albedo = true
	sm.emission_enabled = true
	sm.emission = Color(1.0, 0.2, 0.15)
	sm.emission_energy_multiplier = 3.0
	bites_mm = MultiMesh.new()
	bites_mm.transform_format = MultiMesh.TRANSFORM_3D
	bites_mm.use_colors = true
	bites_mm.mesh = spark
	var bi := MultiMeshInstance3D.new()
	bi.multimesh = bites_mm
	bi.material_override = sm
	bi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	add_child(bi)
	var quad := QuadMesh.new()
	var rm := ShaderMaterial.new()
	rm.shader = load("res://shaders/ring.gdshader")
	rings_mm = MultiMesh.new()
	rings_mm.transform_format = MultiMesh.TRANSFORM_3D
	rings_mm.mesh = quad
	var ri := MultiMeshInstance3D.new()
	ri.multimesh = rings_mm
	ri.material_override = rm
	ri.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	add_child(ri)

func _build_overlay() -> void:
	layer = CanvasLayer.new()
	layer.layer = 2
	add_child(layer)
	profile = Control.new()
	profile.position = Vector2(12, 430)
	profile.size = Vector2(330, 360)
	profile.mouse_filter = Control.MOUSE_FILTER_IGNORE
	profile.draw.connect(_draw_profile)
	layer.add_child(profile)
	chart = Control.new()
	chart.anchor_left = 0.5
	chart.anchor_right = 0.5
	chart.anchor_top = 1.0
	chart.anchor_bottom = 1.0
	chart.offset_left = -330
	chart.offset_right = 330
	chart.offset_top = -132
	chart.offset_bottom = -12
	chart.mouse_filter = Control.MOUSE_FILTER_IGNORE
	chart.draw.connect(_draw_chart)
	layer.add_child(chart)

## Lever panel (garden only), under the goal banner: the body panel owns the left column.
func _build_levers() -> void:
	lever_panel = load("res://scripts/lever_panel.gd").new()
	lever_panel.setup("Your world's environment (L to hide)", LEVERS, _apply_lever,
		func() -> Dictionary: return sim.levers() if ok and not sim.busy() else {},
		"Each is a real rate in the model, x1/4 to x4. Run an experiment: change one, press T, read the report.")
	lever_panel.set_anchors_preset(Control.PRESET_CENTER_TOP)
	lever_panel.offset_left = -180
	lever_panel.offset_top = 124
	layer.add_child(lever_panel)
	_show_levers()

## False while the reef is on the worker (sliders are locked then, see `_lock_levers`).
func _apply_lever(key: String, mult: float) -> bool:
	if not ok or sim.busy():
		return false
	sim.set_lever(key, mult)
	if campaign:
		campaign.state["levers"] = sim.levers()
	wave_mult = sim.levers().get("waves", 1.0)
	return true

## Set one lever and show it (capture flag --levers=key:mult).
func _set_lever(key: String, mult: float) -> void:
	_apply_lever(key, mult)
	_show_levers()

func _show_levers() -> void:
	if lever_panel == null or not ok or sim.busy():
		return
	wave_mult = sim.levers().get("waves", 1.0)
	lever_panel.refresh()

func _lock_levers(on: bool) -> void:
	if lever_panel:
		lever_panel.lock(on)

## Levers saved with the garden come back with it.
func _restore_levers() -> void:
	for key in campaign.state.get("levers", {}):
		sim.set_lever(key, float(campaign.state.levers[key]))
	_show_levers()

## Non-default levers, for report titles.
func _lever_note() -> String:
	if not garden or lever_panel == null or not ok or sim.busy():
		return ""
	return lever_panel.note()

func activate(on: bool) -> void:
	active = on
	visible = on
	set_process(on)
	set_process_unhandled_input(on)
	layer.visible = on
	ff.visible = on
	ff.set_process_unhandled_input(on)
	# the 2D scales draw over an empty world: no sky, no 3D camera behind them
	rig.set_active(on)
	if on:
		cam.make_current()
	else:
		cam.clear_current(false)
	if garden and ok and campaign:
		if on:
			_populate()
		else:
			save_garden()

## Garden: bring back what lived here last session, then release creatures kept since.
func _populate() -> void:
	if not restored:
		restored = true
		_restore_levers()
		for g in campaign.state.garden:
			var n: int = clamp(int(g.cells) / 10, 4, 24)
			var c: int = sim.release(PackedByteArray(g.genome), PackedInt64Array(g.mass), n)
			if c >= 0:
				names[c] = g.name
				if not str(g.name).begins_with("Wild-"):
					mine[c] = true
			elif not str(g.name).begins_with("Wild-"):
				unplaced.append(g)
		# time since the garden last ran (closed game, or other scales before quitting) becomes
		# days of evolution; creatures kept since arrive after it
		var away_h: float = (campaign.session_start - float(campaign.state.get("saved_at", 0.0))) / 3600.0
		var days: float = min(away_h * AWAY_DAYS_PER_HOUR, AWAY_MAX_DAYS)
		if not names.is_empty() and campaign.state.has("saved_at") and days >= 5.0:
			start_ff(days, "While you were away (%s), your world ran %d days%s" % [_hours(away_h), int(days), _lever_note()])
			if sim.busy():
				return
	# back mid fast-forward: the reef is on the worker, `_ff_done` releases them
	if sim.busy():
		return
	_release_kept()

static func _hours(h: float) -> String:
	if h < 1.0:
		return "%d min" % int(h * 60.0)
	if h < 48.0:
		return "%.0f h" % h
	return "%.0f days" % (h / 24.0)

func _release_kept() -> void:
	var arrived := []
	for k in campaign.state.kept:
		if k.get("released", false):
			continue
		var c: int = sim.release(PackedByteArray(k.genome), PackedInt64Array(k.mass), RELEASE_SPORES)
		if c >= 0:
			k.released = true
			names[c] = k.name
			mine[c] = true
			arrived.append(k.name)
		else:
			campaign.toast.emit("Could not release " + k.name, "No room on the lower shore right now; it waits for your next visit.", Color(0.95, 0.6, 0.5))
	if not arrived.is_empty():
		# one write: the released flags and the lineages they became
		save_garden()
		campaign.toast.emit("Released into your world", ", ".join(PackedStringArray(arrived)), Color(0.75, 1.0, 0.55))

func start_ff(days: float, title: String) -> void:
	if not ok or sim.busy():
		return
	if sim.fast_forward(days, DT):
		# the reef is on the worker: levers cannot reach it until it is back
		_lock_levers(true)
		ff_title = title
		selected = -1
		organism = {}
		creatures = PackedInt64Array()
		rings_mm.instance_count = 0

## Captures: fast-forward and wait for the report on the main thread.
func ff_wait(days: float) -> void:
	start_ff(days, "Fast-forward: %d days" % int(days))
	while sim.busy():
		var r: Dictionary = sim.poll_fast_forward()
		if not r.is_empty():
			_ff_done(r)
			break
		OS.delay_msec(50)

func _lineage_name(c: int) -> String:
	return names.get(c, "#%d" % c)

## The report: what changed, in words, from the numbers the worker measured.
func _ff_report(r: Dictionary) -> String:
	var b: Dictionary = r.before
	var a: Dictionary = r.after
	var t := "[b]%s[/b]\n[color=#9ab]day %d to day %d[/color]\n\n" % [ff_title, int(b.t), int(a.t)]
	t += "[b]Population.[/b] %d cells -> %d. Biggest body %d -> %d cells.\n" % [b.cells, a.cells, b.biggest, a.biggest]
	t += "[b]Evolution.[/b] Deepest spore lineage %d -> %d generations; %d -> %d different genomes alive. Each generation is a chance for copy mistakes, and the environment keeps the ones that work.\n\n" % [b.max_generation, a.max_generation, b.genomes, a.genomes]
	var before := {}
	var bl: PackedInt64Array = b.lineages
	for i in range(0, bl.size(), 2):
		before[bl[i]] = bl[i + 1]
	var al: PackedInt64Array = a.lineages
	t += "[b]Lineages[/b] (%d alive):\n" % (al.size() / 2)
	for i in range(0, min(al.size(), 24), 2):
		var c: int = al[i]
		var was: int = before.get(c, 0)
		var arrow := "new" if was == 0 else ("up" if al[i + 1] > was * 1.2 else ("down" if al[i + 1] < was * 0.8 else "steady"))
		t += "  %s: %d -> %d cells (%s)\n" % [_lineage_name(c), was, al[i + 1], arrow]
	var lost: PackedInt64Array = r.lost
	if lost.size() > 0:
		var gone := []
		for c in lost:
			gone.append(_lineage_name(c))
		var more := "" if gone.size() <= 12 else " and %d more" % (gone.size() - 12)
		t += "[color=#f9a]Extinct (%d): %s%s[/color]\n" % [gone.size(), ", ".join(PackedStringArray(gone.slice(0, 12))), more]
	t += "\n[b]What the bodies do[/b] (mean over all cells):\n"
	var tb: PackedFloat32Array = b.traits
	var ta: PackedFloat32Array = a.traits
	var shifts := 0
	for k in 5:
		if tb[k] <= 0.001:
			continue
		var d: float = (ta[k] - tb[k]) / tb[k]
		if abs(d) >= 0.08:
			shifts += 1
			t += "  %s %+.0f%%: %s\n" % [TRAIT_NAMES[k], d * 100.0, TRAIT_WHY[k] if d > 0 else "less of it pays here now"]
	if tb[0] + tb[1] + tb[2] <= 0.003:
		t += "  nothing to compare yet: it started as spores that had not switched any genes on.\n"
	elif shifts == 0:
		t += "  no trait moved by more than 8%: the reef is near a balance for now.\n"
	t += "\n[b]Creatures[/b] (20+ cells, 3+ jobs, crawling): seen on %d noons in %d days, at most %d at once.\n" % [r.creature_days, int(round(r.days)), r.peak_creatures]
	return t

func _ff_done(r: Dictionary) -> void:
	if r.has("failed"):
		ok = false
		ff.close()
		if campaign:
			campaign.toast.emit("The reef crashed while fast-forwarding", "Restart the viewer; your world's last save is intact.", Color(0.95, 0.5, 0.5))
		return
	if garden:
		_lock_levers(false)
		_show_levers()
	var text := _ff_report(r)
	ff.show_report(text + "\n[color=#9ab]Any key to close. Saved in the codex (J).[/color]")
	timer = 0.0
	if campaign:
		campaign.field_note(ff_title, text)
	if garden:
		_release_kept()
		save_garden()

## Garden: the surviving lineages, one representative each, to come back next session.
func save_garden() -> void:
	# before the first visit the sim is empty: saving would wipe last session's garden
	if not garden or not ok or campaign == null or not restored or sim.busy():
		return
	# the player's own lineages (`mine`) survive a save however small, and so do ones that
	# could not be placed this session; wild offshoots only the biggest few
	var out := []
	var wild := 0
	for l in sim.lineages(100000):
		if not mine.has(int(l.lineage)):
			wild += 1
			if wild > GARDEN_KEEP:
				continue
			# lineage numbers restart each session: wild names come from a saved counter
			if not names.has(int(l.lineage)):
				campaign.state.wild = int(campaign.state.get("wild", 0)) + 1
				names[int(l.lineage)] = "Wild-%d" % campaign.state.wild
		out.append({"name": names[int(l.lineage)], "genome": Array(l.genome), "mass": Array(l.mass), "cells": l.cells})
	out.append_array(unplaced)
	campaign.state.garden = out
	campaign.state.saved_at = Time.get_unix_time_from_system()
	campaign.save()

## Keep the selected body: its genome goes to the player's world.
func keep_selected() -> void:
	if campaign == null or organism.is_empty() or sim.busy():
		return
	var name: String = campaign.keep(organism, "your world" if garden else "reef run %d" % campaign.state.run)
	if name == "":
		return
	var what := "a creature" if organism.creature else "%d cells, %d jobs" % [organism.cells, organism.jobs]
	campaign.toast.emit("Kept " + name + " (" + what + ")", "It will be released in your world (8 in the hub). Keep several and see who survives.", Color(0.75, 1.0, 0.55))

## A fresh reef from the same founders with new dice: another try at a creature.
func new_run() -> void:
	if campaign == null or garden or sim.busy():
		return
	var run: int = campaign.new_run()
	for id in skins:
		skins[id].queue_free()
	skins.clear()
	ok = sim.start(ProjectSettings.globalize_path(RATES), ProjectSettings.globalize_path(SEED), run)
	selected = -1
	organism = {}
	history = PackedFloat32Array()
	if ok:
		sim.step(1, DT)
	campaign.toast.emit("Run %d" % run, "A new reef from the same founders. Different dice, different history.", Color(1.0, 0.5, 0.65))

func _notification(what: int) -> void:
	if what == NOTIFICATION_WM_CLOSE_REQUEST and active:
		save_garden()

func explain_text() -> String:
	return GARDEN_EXPLAIN if garden else EXPLAIN

func speed() -> int:
	return SPEEDS[speed_idx]

func warm(n: int) -> void:
	sim.step(n, DT)

func selected_info() -> Dictionary:
	return {}

func set_lineage(on: bool) -> void:
	by_lineage = on
	cell_mat.set_shader_parameter("by_lineage", 1.0 if on else 0.0)

## Select the biggest body and fly to it (capture flag --pick).
func pick_longest() -> void:
	selected = sim.biggest()
	if selected >= 0:
		target = sim.organism_center(selected)
		dist = 45.0
		organism = sim.organism(selected)

func _process(delta: float) -> void:
	if not ok:
		return
	if sim.busy():
		var r: Dictionary = sim.poll_fast_forward()
		if r.is_empty():
			ff.show_progress("%s   %d%%" % [ff_title, int(sim.fast_forward_progress() * 100.0)], sim.fast_forward_progress())
			return
		_ff_done(r)
	if speed() > 0:
		acc += speed() * DAYS_PER_UNIT * delta
		var n := int(acc / DT)
		if n > 0:
			acc -= n * DT
			sim.step(min(n, 400), DT)
	env = sim.env()
	rig.update(env, cam.global_position.y, water.position.y)
	var lvl: float = sim.water_level()
	water.position.y = lvl
	var day: float = clamp(env.visible * 1.4 + 0.08, 0.0, 1.0)
	# the waves lever shows in the sea: calm water is flat, storms heave
	water_mat.set_shader_parameter("swell", (0.25 + 1.4 * env.cloud * env.cloud) * sqrt(wave_mult))
	water_mat.set_shader_parameter("daylight", day)
	rock_mat.set_shader_parameter("water_level", lvl)
	rock_mat.set_shader_parameter("daylight", day)
	cell_mat.set_shader_parameter("sway", 0.3 + 1.5 * env.cloud)
	cell_mat.set_shader_parameter("daylight", day)
	var buf: PackedFloat32Array = sim.cell_buffer(selected)
	cells_mm.instance_count = buf.size() / 20
	if buf.size() > 0:
		cells_mm.buffer = buf
	_update_bites()
	if auto_orbit and not orbiting and not panning:
		yaw += delta * 0.03
	_update_camera()
	timer -= delta
	if timer <= 0.0:
		timer = 0.5
		if garden:
			save_timer -= 0.5
			if save_timer <= 0.0:
				save_timer = 30.0
				save_garden()
		stats = sim.stats()
		history = sim.history()
		var rows := history.size() / 11
		if rows > 0:
			var b := (rows - 1) * 11
			cell_mat.set_shader_parameter("mean_expr", Vector4(history[b + 5], history[b + 6], history[b + 7], history[b + 8]))
			cell_mat.set_shader_parameter("mean_adh", history[b + 9])
		if selected >= 0:
			organism = sim.organism(selected)
			if organism.is_empty():
				selected = -1
		_update_rings()
		_update_skins()
		profile.queue_redraw()
		chart.queue_redraw()

## Skins for the selected body and every creature: rebuilt on the half-second tick (bodies
## grow, tear and crawl), dropped when the body is gone or no longer shown.
func _update_skins() -> void:
	if skin_mat == null:
		# opaque: the water reads the screen after the opaque pass, so a translucent skin
		# under the surface would vanish
		skin_mat = StandardMaterial3D.new()
		skin_mat.vertex_color_use_as_albedo = true
		skin_mat.roughness = 0.22
		skin_mat.metallic_specular = 0.7
		skin_mat.rim_enabled = true
		skin_mat.rim = 0.6
		skin_mat.rim_tint = 0.3
	var want := {}
	if skin_all:
		for id in sim.body_ids(SKIN_MIN):
			want[id] = true
	for id in creatures:
		want[id] = true
	if selected >= 0:
		want[selected] = true
	for id in skins.keys():
		if not want.has(id):
			skins[id].queue_free()
			skins.erase(id)
	# the selected body and creatures every tick, whatever the cost; then bodies with no skin
	# yet and the rest in turns, within the time budget
	var first := []
	if selected >= 0:
		first.append(selected)
	for id in creatures:
		if id != selected:
			first.append(id)
	for id in first:
		_build_skin(id)
	var ids: Array = want.keys()
	var todo := []
	for id in ids:
		if not skins.has(id):
			todo.append(id)
	if ids.size() > 0:
		for i in SKIN_BATCH:
			todo.append(ids[(skin_turn + i) % ids.size()])
		skin_turn = (skin_turn + SKIN_BATCH) % ids.size()
	var done := {}
	for id in first:
		done[id] = true
	var t0 := Time.get_ticks_usec()
	for id in todo:
		if done.has(id):
			continue
		if Time.get_ticks_usec() - t0 > SKIN_BUDGET_US:
			break
		done[id] = true
		_build_skin(id)

func _build_skin(id: int) -> void:
	var pts: PackedVector3Array = sim.body_points(id)
	var shape: PackedFloat32Array = sim.body_shape(id)
	var n: int = min(pts.size(), shape.size() / 8)
	if n < 3:
		if skins.has(id):
			skins[id].queue_free()
			skins.erase(id)
		return
	var mean := [0.0, 0.0, 0.0, 0.0, 0.0]
	for i in n:
		for k in 5:
			mean[k] += shape[i * 8 + 3 + k] / n
	var cols := PackedColorArray()
	for i in n:
		cols.append(Gallery.tint([shape[i * 8 + 3], shape[i * 8 + 4], shape[i * 8 + 5], shape[i * 8 + 6], shape[i * 8 + 7]], mean))
	var arrays: Array = BodySkin.build(pts.slice(0, n), cols)
	if arrays.size() != Mesh.ARRAY_MAX:
		return
	var am := ArrayMesh.new()
	am.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)
	if not skins.has(id):
		var mi := MeshInstance3D.new()
		mi.material_override = skin_mat
		mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		add_child(mi)
		skins[id] = mi
	skins[id].mesh = am

## Gold rings over bodies that meet the creature goal right now.
func _update_rings() -> void:
	creatures = sim.creatures()
	rings_mm.instance_count = creatures.size()
	for i in creatures.size():
		var c: Vector3 = sim.organism_center(creatures[i])
		rings_mm.set_instance_transform(i, Transform3D(Basis().scaled(Vector3.ONE * 9.0), c + Vector3(0, 4.0, 0)))

## Step until a creature exists (at most `max_steps`), then fly to it. Captures only.
func hunt(max_steps: int) -> void:
	# creatures show around midday (jobs come from light-switched genes): check at noon
	var t: float = sim.t_days()
	var n := int(round((ceil(t - 0.5) + 0.5 - t) / DT))
	sim.step(n, DT)
	while n < max_steps and sim.creatures().is_empty():
		sim.step(50, DT)
		n += 50
	_update_rings()
	next_creature()

## Fly to the next creature and select it.
func next_creature() -> void:
	if creatures.is_empty():
		if campaign:
			campaign.toast.emit("No creature right now", "They come and go, mostly around midday when light switches cells' jobs. Speed up time (4, 5) and watch for gold rings.", Color(0.8, 0.85, 0.95))
		return
	creature_at = (creature_at + 1) % creatures.size()
	selected = creatures[creature_at]
	organism = sim.organism(selected)
	target = sim.organism_center(selected)
	dist = 45.0
	profile.queue_redraw()

func _update_bites() -> void:
	var b: PackedFloat32Array = sim.bites(0.6)
	var n := b.size() / 4
	bites_mm.instance_count = n
	for i in n:
		var age: float = b[i * 4 + 3]
		var s: float = 0.25 + 1.0 * age
		bites_mm.set_instance_transform(i, Transform3D(Basis().scaled(Vector3(s, s, s)), Vector3(b[i * 4], b[i * 4 + 1], b[i * 4 + 2])))
		bites_mm.set_instance_color(i, Color(1.0, 0.25, 0.15, clamp(1.0 - age / 0.6, 0.0, 1.0) * 0.55))

func _update_camera() -> void:
	pitch = clamp(pitch, -1.45, 0.25)
	dist = clamp(dist, 6.0, 900.0)
	var off := Vector3(sin(yaw) * cos(pitch), -sin(pitch), cos(yaw) * cos(pitch)) * dist
	cam.position = target + off
	cam.look_at(target, Vector3.UP)

func _unhandled_input(e: InputEvent) -> void:
	if e is InputEventMouseButton:
		if e.button_index == MOUSE_BUTTON_WHEEL_UP and e.pressed:
			dist *= 0.88
		elif e.button_index == MOUSE_BUTTON_WHEEL_DOWN and e.pressed:
			dist /= 0.88
		elif e.button_index == MOUSE_BUTTON_RIGHT:
			orbiting = e.pressed and not e.shift_pressed
			panning = e.pressed and e.shift_pressed
		elif e.button_index == MOUSE_BUTTON_MIDDLE:
			panning = e.pressed
		elif e.button_index == MOUSE_BUTTON_LEFT and e.pressed:
			var id: int = sim.pick(cam.project_ray_origin(e.position), cam.project_ray_normal(e.position))
			selected = id
			organism = sim.organism(id) if id >= 0 else {}
			profile.queue_redraw()
	elif e is InputEventMouseMotion:
		if orbiting:
			yaw -= e.relative.x * 0.005
			pitch -= e.relative.y * 0.005
		elif panning:
			var right := cam.global_transform.basis.x
			var fwd := Vector3(-sin(yaw), 0.0, -cos(yaw))
			target += (-right * e.relative.x + fwd * e.relative.y) * dist * 0.0015
	elif e is InputEventKey and e.pressed:
		match e.keycode:
			KEY_SPACE:
				speed_idx = 0 if speed_idx != 0 else 2
			KEY_1, KEY_2, KEY_3, KEY_4, KEY_5:
				speed_idx = e.keycode - KEY_1
			KEY_F:
				if selected >= 0:
					target = sim.organism_center(selected)
					dist = 40.0
			KEY_O:
				auto_orbit = not auto_orbit
			KEY_C:
				set_lineage(not by_lineage)
			KEY_B:
				pick_longest()
			KEY_K:
				if not e.echo:
					keep_selected()
			KEY_N:
				if not e.echo:
					new_run()
			KEY_G:
				if not e.echo:
					next_creature()
			KEY_S:
				if not e.echo:
					skin_all = not skin_all
					_update_skins()
			KEY_T:
				if not e.echo:
					var days: float = FF_DAYS * (5.0 if e.shift_pressed else 1.0)
					start_ff(days, "Fast-forward: %d days%s" % [int(days), _lever_note()])
			KEY_L:
				if garden and not e.echo:
					lever_panel.visible = not lever_panel.visible
					_show_levers()
			KEY_ESCAPE:
				selected = -1
				organism = {}

## Campaign goals (core goal.rs). Reef: find and keep a creature (the keep completes it).
## Garden: one lineage makes creatures on enough days of the window.
func goal_progress() -> Dictionary:
	# while fast-forwarding the reef is on the worker: no numbers to read
	var g: Dictionary = sim.goal() if ok and not sim.busy() else {}
	if g.is_empty():
		return {}
	var near_c: int = g.get("near_cells", 0)
	var near_j: int = g.get("near_jobs", 0)
	if garden:
		var who: String = names.get(int(g.won_lineage), "lineage %d" % g.won_lineage)
		var lead: String = names.get(int(g.best_lineage), "none yet")
		return {"p": min(1.0, float(g.best_days) / g.need_days), "done": g.won_lineage >= 0, "args": [g.need_days, int(g.window_days)],
			"now": "best: %s, creatures on %d of the last %d days" % [lead, g.best_days, int(g.window_days)], "who": who + " in your world"}
	var body: float = 0.5 * min(1.0, float(near_c) / g.min_cells) + 0.5 * min(1.0, float(near_j) / g.min_jobs)
	var now := "best crawler %d cells, %d jobs" % [near_c, near_j]
	if g.qualifying > 0:
		now = "%d creature(s) on the reef now: G to find one, K to keep it" % g.qualifying
	return {"p": 0.9 * body, "done": false, "args": [g.min_cells, g.min_jobs], "now": now}

func hud_text() -> String:
	if not ok:
		return "[color=#f88]data/reef_seed.json or rates0.json missing: run primordium reef-seed[/color]"
	if stats.is_empty():
		return ""
	var t := "[b]%s[/b]  day %.0f\ncells %d in %d bodies (%d multicellular)\ncrawling: %d bodies (%d multicellular)\nbiggest %d cells   tallest %d   generation %d\nadrift %d   spores %d   tears %d   bites %d\nspore bit error %.2f%%\n" % [
		"Your world" if garden else "Reef, run %d" % (campaign.state.run if campaign else 1), env.get("t_days", 0.0), stats.cells, stats.bodies, stats.multicellular, stats.crawlers, stats.crawlers_multi, stats.biggest, stats.tallest, stats.generation,
		stats.adrift, stats.spores, stats.tears, stats.bites, stats.bit_error * 100.0]
	if not by_lineage:
		t += "[color=#9ab]tint (C for lineage): [color=#40ccd9]polymerase[/color] [color=#59cc4d]metabolism[/color] [color=#f2e0b8]membrane[/color] [color=#bf3852]pigment[/color] [color=#ffbf33]glue[/color]\nglowing on the surface: spores and torn pieces adrift\nshimmering with a green rim: a crawling body\nred flash: a bite[/color]\n"
	if garden:
		t += "lineages alive %d; largest: " % stats.lineages
	else:
		t += "lineages alive %d of %d founders; largest: " % [stats.lineages, stats.founders]
	var parts := []
	for v in stats.top_lineages:
		parts.append("%s %.0f%%" % [names.get(int(v.x), "#%d" % int(v.x)), v.y * 100.0])
	t += ", ".join(PackedStringArray(parts)) + "\n"
	if by_lineage:
		t += "[color=#9ab]tint: lineage (C for jobs)[/color]\n"
	if stats.unsure > 0:
		t += "[color=#f9c]%d cells where the coast's table is thin[/color]\n" % stats.unsure
	t += "[b]Discoveries[/b]\n"
	for d in stats.discoveries:
		t += "[color=#cbf]" + d + "[/color]\n"
	if garden and stats.cells == 0:
		t += "[color=#f9c]Nothing alive here. Keep creatures at the reef (K) to release them.[/color]\n"
	t += "\n[color=#789]Tab: switch scale, J codex\nright-drag orbit, shift/middle-drag pan, wheel zoom\nleft-click a body, F fly to it, B biggest, Esc clear\nG next creature (gold ring), K keep the selected body%s\nS smooth skins on every body\nT fast-forward 100 days (Shift+T 500) with a report%s\nO auto-orbit, C lineage colours, space pause, 1-5 speed[/color]" % ["" if garden else ", N new run", "\nL environment levers" if garden else ""]
	return t

## Selected body: its make-up layer by layer (rock at the bottom) and its genes' switches.
func _draw_profile() -> void:
	if organism.is_empty():
		return
	var font := ThemeDB.fallback_font
	var sz := profile.size
	profile.draw_rect(Rect2(Vector2.ZERO, sz), Color(0.03, 0.05, 0.08, 0.82))
	profile.draw_string(font, Vector2(10, 18), "Body #%d  lineage %d  %d cells, %d tall  gen %d  %s" % [
		organism.id, organism.lineage, organism.cells, organism.height, organism.generation, "adrift" if organism.adrift else "age %.0f d" % organism.age], HORIZONTAL_ALIGNMENT_LEFT, -1, 12, Color(1, 1, 1, 0.9))
	if organism.get("creature", false):
		profile.draw_string(font, Vector2(sz.x - 84, 54), "CREATURE", HORIZONTAL_ALIGNMENT_LEFT, -1, 12, Color(1.0, 0.83, 0.42))
	profile.draw_string(font, Vector2(10, 34), "jobs %d   " % organism.get("jobs", 0) + "depth %.2f   waves %.2f   store %d   %s" % [organism.depth, organism.waves, organism.store, "crawls %.1f cells/day" % organism.speed if organism.speed > 0.0 else "anchored"], HORIZONTAL_ALIGNMENT_LEFT, -1, 11, Color(0.7, 0.85, 1.0, 0.8))
	var layers: PackedFloat32Array = organism.layers
	var nl := layers.size() / 6
	var top := 46.0
	var area_h := 150.0
	var lh: float = min(area_h / max(nl, 1), 18.0)
	var maxn := 1.0
	for z in nl:
		maxn = max(maxn, layers[z * 6])
	profile.draw_string(font, Vector2(10, top + 8), "by height (cells; tint = genes on there)", HORIZONTAL_ALIGNMENT_LEFT, -1, 10, Color(1, 1, 1, 0.6))
	for z in nl:
		var n: float = layers[z * 6]
		var y: float = top + 14.0 + (nl - 1 - z) * lh
		var col := _tint(layers[z * 6 + 1], layers[z * 6 + 2], layers[z * 6 + 3], layers[z * 6 + 4], layers[z * 6 + 5])
		var w: float = 220.0 * n / maxn
		profile.draw_rect(Rect2(Vector2(60.0 + (220.0 - w) * 0.5, y), Vector2(w, lh - 1.0)), col)
		profile.draw_string(font, Vector2(10, y + lh - 3.0), "z%d  %d" % [z, int(n)], HORIZONTAL_ALIGNMENT_LEFT, -1, 9, Color(1, 1, 1, 0.6))
	var gy := top + 14.0 + nl * lh + 22.0
	profile.draw_string(font, Vector2(10, gy), "genes (%d letters)" % organism.genome_len, HORIZONTAL_ALIGNMENT_LEFT, -1, 11, Color(1, 1, 1, 0.75))
	var genes: Array = organism.genes
	for k in genes.size():
		var g: Dictionary = genes[k]
		var x := 10.0 + (k % 2) * 160.0
		var y := gy + 16.0 + (k / 2) * 16.0
		if y > sz.y - 6:
			break
		profile.draw_rect(Rect2(Vector2(x, y - 9), Vector2(10, 10)), ROLE_COLORS[g.role])
		var label: String = ROLE_NAMES[g.role]
		if g.promoter != "":
			label += " [" + g.promoter + "]"
		profile.draw_string(font, Vector2(x + 14, y), label, HORIZONTAL_ALIGNMENT_LEFT, -1, 10, Color(1, 1, 1, 0.8))

func _tint(poly: float, synth: float, amph: float, shade: float, adh: float) -> Color:
	var w := [pow(poly / 0.5, 2.0), pow(synth / 0.5, 2.0), pow(amph / 0.45, 2.0), pow(shade / 0.5, 2.0), pow(adh / 0.45, 2.0)]
	var cols := [ROLE_COLORS[0], ROLE_COLORS[1], ROLE_COLORS[2], ROLE_COLORS[3], ROLE_COLORS[5]]
	var c := Color(0, 0, 0, 0)
	var tot := 0.001
	for k in 5:
		c += cols[k] * w[k]
		tot += w[k]
	c = c / tot
	c.a = 1.0
	return c

## Population over time: cells (white), mean multicellular body size (gold), tallest (cyan).
func _draw_chart() -> void:
	var font := ThemeDB.fallback_font
	var sz := chart.size
	chart.draw_rect(Rect2(Vector2.ZERO, sz), Color(0.03, 0.05, 0.08, 0.82))
	chart.draw_string(font, Vector2(8, 14), "Reef over time: cells (white), mean body size (gold), tallest body (cyan), lineages alive (violet)", HORIZONTAL_ALIGNMENT_LEFT, -1, 11, Color(1, 1, 1, 0.75))
	var rows := history.size() / 11
	if rows < 2:
		return
	var area := Rect2(8, 22, sz.x - 16, sz.y - 30)
	var series := [[1, Color(1, 1, 1, 0.85)], [3, Color(1.0, 0.75, 0.2, 0.9)], [4, Color(0.4, 0.9, 1.0, 0.9)], [10, Color(0.8, 0.5, 1.0, 0.9)]]
	for s in series:
		var k: int = s[0]
		var top := 1.0
		for i in rows:
			top = max(top, history[i * 11 + k])
		var pts := PackedVector2Array()
		for i in rows:
			var x: float = area.position.x + area.size.x * i / (rows - 1)
			var y: float = area.position.y + area.size.y * (1.0 - history[i * 11 + k] / top)
			pts.append(Vector2(x, y))
		chart.draw_polyline(pts, s[1], 1.5, true)
