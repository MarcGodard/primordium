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
const SCALE_NAME := "scale 5: reef"
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

func _ready() -> void:
	sim = ReefSim.new()
	add_child(sim)
	ok = sim.start(ProjectSettings.globalize_path("res://../data/rates0.json"), ProjectSettings.globalize_path("res://../data/reef_seed.json"), 1)
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

func activate(on: bool) -> void:
	active = on
	visible = on
	set_process(on)
	set_process_unhandled_input(on)
	layer.visible = on
	# the 2D scales draw over an empty world: no sky, no 3D camera behind them
	rig.set_active(on)
	if on:
		cam.make_current()
	else:
		cam.clear_current(false)

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
	water_mat.set_shader_parameter("swell", 0.25 + 1.4 * env.cloud * env.cloud)
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
		profile.queue_redraw()
		chart.queue_redraw()

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
			KEY_ESCAPE:
				selected = -1
				organism = {}

func hud_text() -> String:
	if not ok:
		return "[color=#f88]data/reef_seed.json or rates0.json missing: run primordium reef-seed[/color]"
	if stats.is_empty():
		return ""
	var t := "[b]Reef[/b]  day %.0f\ncells %d in %d bodies (%d multicellular)\ncrawling: %d bodies (%d multicellular)\nbiggest %d cells   tallest %d   generation %d\nadrift %d   spores %d   tears %d   bites %d\nspore bit error %.2f%%\n" % [
		env.get("t_days", 0.0), stats.cells, stats.bodies, stats.multicellular, stats.crawlers, stats.crawlers_multi, stats.biggest, stats.tallest, stats.generation,
		stats.adrift, stats.spores, stats.tears, stats.bites, stats.bit_error * 100.0]
	if not by_lineage:
		t += "[color=#9ab]tint (C for lineage): [color=#40ccd9]polymerase[/color] [color=#59cc4d]metabolism[/color] [color=#f2e0b8]membrane[/color] [color=#bf3852]pigment[/color] [color=#ffbf33]glue[/color]\nglowing on the surface: spores and torn pieces adrift\nshimmering with a green rim: a crawling body\nred flash: a bite[/color]\n"
	t += "lineages alive %d of %d founders; largest: " % [stats.lineages, stats.founders]
	var parts := []
	for v in stats.top_lineages:
		parts.append("#%d %.0f%%" % [int(v.x), v.y * 100.0])
	t += ", ".join(PackedStringArray(parts)) + "\n"
	if by_lineage:
		t += "[color=#9ab]tint: lineage (C for jobs)[/color]\n"
	if stats.unsure > 0:
		t += "[color=#f9c]%d cells where the coast's table is thin[/color]\n" % stats.unsure
	t += "[b]Discoveries[/b]\n"
	for d in stats.discoveries:
		t += "[color=#cbf]" + d + "[/color]\n"
	t += "\n[color=#789]Tab: switch scale\nright-drag orbit, shift/middle-drag pan, wheel zoom\nleft-click a body, F fly to it, B biggest, Esc clear\nO auto-orbit, C lineage colours, space pause, 1-5 speed[/color]"
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
	profile.draw_string(font, Vector2(10, 34), "depth %.2f   waves %.2f   store %d   %s" % [organism.depth, organism.waves, organism.store, "crawls %.1f cells/day" % organism.speed if organism.speed > 0.0 else "anchored"], HORIZONTAL_ALIGNMENT_LEFT, -1, 11, Color(0.7, 0.85, 1.0, 0.8))
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
