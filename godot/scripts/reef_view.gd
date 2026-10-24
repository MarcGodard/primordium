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
## Creatures' skins: their own colours with a gold glow along the silhouette.
var creature_mat: ShaderMaterial
## Where each creature's marker floats (above its top), set on the half-second tick; scaled
## every frame to one size on screen.
var marker_at := PackedVector3Array()
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
## Shift+G: run at full speed until a creature appears (days searched so far, -1: off). A count,
## not a start day: new runs and fast-forwards move the clock.
var search_from := -1.0
## Days a search runs before it gives up.
const SEARCH_DAYS := 400.0
## A creature was on the reef at the last tick: a new one stops the clock once per sighting.
var had_creature := false
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
## The archipelago (scale 6): islands with their own shores, one shown at a time.
var arch := false
## Last `sim.archipelago()` (refreshed with the stats).
var arch_state := {}
## Kept creatures already put on the islands this session, by name.
var arch_released := {}
## Seed of the current archipelago (N starts the next).
var arch_seed := 11
var terrain_mi: MeshInstance3D

const ARCH_LEVERS := [
	["currents", "Currents", "How often a drifting spore is carried to another island. Strong currents mix the islands; weak ones leave them isolated."],
]
const ARCH_EXPLAIN := """[b]Scale 6: an archipelago[/b]

[b]What you see.[/b] Seven islands of different sizes under one sky, each with its own shore: the calm [b]Lagoon[/b], surf-beaten [b]Surf Point[/b], the sunlit shallow [b]Sun Flats[/b], dark [b]Deepwater[/b] and more, out to tiny [b]Lone Stack[/b]. Every island starts with wild life from the same coast; the creatures you kept land on the Lagoon. You stand on one island at a time; the others sit on the horizon where they lie on the map (I sails to the next).

[b]Plants and animals.[/b] Bodies that stay put and grow on light are the islands' [b]plants[/b] (seaweeds: the first plants were algae). Bodies that crawl or bite are its [b]animals[/b]. Each shore keeps different ones: calm water grows tall seaweed, surf keeps low crusts and crawlers.

[b]Goal.[/b] A living web on every island at once. Small, far and harsh islands hold the fewest kinds and lose them most easily ([b]island biogeography[/b]); spores from neighbours restock them.

[b]Try.[/b] L opens the Currents lever: strong currents carry more spores further. Weak ones leave islands isolated, and an isolated lineage can split into two kinds (a discovery for the codex). T fast-forwards, N starts a new archipelago."""
var campaign: Node
## Garden lineage number -> creature name.
var names := {}
## Garden lineages that are the player's creatures (kept, not wild offshoots): always saved.
var mine := {}
## Garden lineage number -> genus: kept from the creature it began with, so a body kept again
## from that lineage is named in the same genus.
var genera := {}
## Today's expedition (YYYYMMDD) while this reef is one, else 0.
var expedition := 0
## Saved lineages that could not be released this session: carried over to the next save.
var unplaced := []
var save_timer := 0.0
var restored := false
var RATES: String = "res://../" + ElementsSim.data_dir() + "rates0.json"
var SEED: String = "res://../" + ElementsSim.data_dir() + "reef_seed.json"
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
## Chart page: false population, true mean traits. Click the chart to switch.
var chart_traits := false
## Lever changes this session as [day, text, key], drawn as markers on the chart.
var lever_marks := []
var wave_mult := 1.0
## Unnamed lineages a garden save keeps (biggest first); named ones are always kept.
const GARDEN_KEEP := 32

func _ready() -> void:
	sim = ReefSim.new()
	add_child(sim)
	if garden:
		SCALE_NAME = "your world"
		ok = sim.start_garden(ProjectSettings.globalize_path(RATES), ProjectSettings.globalize_path(SEED), 7)
	elif arch:
		SCALE_NAME = "scale 6: archipelago"
		ok = sim.start_archipelago(ProjectSettings.globalize_path(RATES), ProjectSettings.globalize_path(SEED), arch_seed)
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
	cam.far = 12000.0
	add_child(cam)
	_build_overlay()
	ff = load("res://scripts/ff_panel.gd").new()
	add_child(ff)
	if garden or arch:
		_build_levers()
	if ok:
		sim.step(1, DT)

func _build_environment() -> void:
	rig = load("res://scripts/sky_rig.gd").new()
	add_child(rig)

## Rock from the sim's height grid, inside a wider made-up shore so the reef looks whole from
## any angle: the coast runs on along the shore, the seafloor falls away, hills rise inland.
## One mesh: the grid is exact over the reef and coarsens outward, so the seams cannot crack.
func _build_terrain() -> void:
	var h: PackedFloat32Array = sim.heights()
	var w := int(extent.x)
	var d := int(extent.y)
	if h.size() != (w + 1) * (d + 1):
		return
	var hmin := INF
	for v in h:
		hmin = min(hmin, v)
	# mean height of each row up the beach: what the coast looks like away from the reef
	var row := PackedFloat32Array()
	row.resize(d + 1)
	for y in d + 1:
		var sum := 0.0
		for x in w + 1:
			sum += h[y * (w + 1) + x]
		row[y] = sum / (w + 1)
	ground = [h, row, w, d, hmin]
	var xs := _axis(w, SURROUND, SURROUND)
	var ys := _axis(d, SURROUND, SURROUND * 0.7)
	var nx := xs.size()
	var ny := ys.size()
	var verts := PackedVector3Array()
	verts.resize(nx * ny)
	for j in ny:
		for i in nx:
			verts[j * nx + i] = Vector3(xs[i], _ground(xs[i], ys[j], h, row, w, d, hmin), -ys[j])
	var norms := PackedVector3Array()
	norms.resize(verts.size())
	for j in ny:
		for i in nx:
			var ex: Vector3 = verts[j * nx + min(i + 1, nx - 1)] - verts[j * nx + max(i - 1, 0)]
			var ey: Vector3 = verts[min(j + 1, ny - 1) * nx + i] - verts[max(j - 1, 0) * nx + i]
			norms[j * nx + i] = ex.cross(ey).normalized()
	var idx := PackedInt32Array()
	for j in ny - 1:
		for i in nx - 1:
			var a := j * nx + i
			var c := a + nx
			idx.append_array([a, c, a + 1, a + 1, c, c + 1])
	var arrays := []
	arrays.resize(Mesh.ARRAY_MAX)
	arrays[Mesh.ARRAY_VERTEX] = verts
	arrays[Mesh.ARRAY_NORMAL] = norms
	arrays[Mesh.ARRAY_INDEX] = idx
	var mesh := ArrayMesh.new()
	mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)
	if rock_mat == null:
		rock_mat = ShaderMaterial.new()
		rock_mat.shader = load("res://shaders/rock.gdshader")
	# an island switch builds the next island's rock in its place
	if terrain_mi:
		terrain_mi.queue_free()
	terrain_mi = MeshInstance3D.new()
	terrain_mi.mesh = mesh
	terrain_mi.material_override = rock_mat
	add_child(terrain_mi)

## [heights, row profile, w, d, lowest]: what `_ground` needs, kept for the camera.
var ground := []

## How far the made-up shore runs past the reef, world units.
const SURROUND := 1100.0

## Grid lines along one axis: every unit over the reef (0..n), then steps growing outward to
## `lo` before it and `hi` after it.
func _axis(n: int, lo: float, hi: float) -> PackedFloat32Array:
	var before := []
	var t := 0.0
	var step := 1.0
	while t < lo:
		t += step
		before.push_front(-t)
		step = min(step * 1.18, 60.0)
	var out := PackedFloat32Array(before)
	for k in n + 1:
		out.append(float(k))
	t = float(n)
	step = 1.0
	while t < n + hi:
		t += step
		out.append(t)
		step = min(step * 1.18, 60.0)
	return out

## Ground height anywhere: the sim's own over the reef; outside, the nearest edge blended toward
## the coast's mean profile along the shore, falling to deep floor seaward, rising inland.
func _ground(x: float, y: float, h: PackedFloat32Array, row: PackedFloat32Array, w: int, d: int, hmin: float) -> float:
	var cx: float = clamp(x, 0.0, w)
	var cy: float = clamp(y, 0.0, d)
	# bilinear: the mesh only asks at integer corners, the camera anywhere
	var x0 := mini(int(cx), w - 1)
	var y0 := mini(int(cy), d - 1)
	var tx := cx - x0
	var ty := cy - y0
	var r0: float = lerp(h[y0 * (w + 1) + x0], h[y0 * (w + 1) + x0 + 1], tx)
	var r1: float = lerp(h[(y0 + 1) * (w + 1) + x0], h[(y0 + 1) * (w + 1) + x0 + 1], tx)
	var base: float = lerp(r0, r1, ty)
	if x >= 0.0 and x <= w and y >= 0.0 and y <= d:
		return base
	var prof: float = lerp(row[y0], row[y0 + 1], ty)
	if arch:
		return _island_ground(x, y, base, w, d, hmin)
	var dx: float = max(-x, x - w, 0.0)
	var dy_sea: float = max(-y, 0.0)
	var dy_land: float = max(y - d, 0.0)
	var far: float = max(dx, max(dy_sea, dy_land))
	# rolling relief grows with distance so the edge of the reef never shows as a line
	var relief := (_vnoise(x * 0.012, y * 0.012) - 0.5) * 14.0 + (_vnoise(x * 0.04, y * 0.04) - 0.5) * 4.0
	var g: float = lerp(base, prof, smoothstep(0.0, 120.0, dx))
	g += relief * smoothstep(0.0, 80.0, far)
	g = lerp(g, hmin - 30.0 + relief * 0.5, smoothstep(0.0, 260.0, dy_sea))
	g += 18.0 * smoothstep(0.0, 400.0, dy_land) + relief * 0.8 * smoothstep(0.0, 120.0, dy_land)
	# the far rim sinks under the sea: an island, never a cut edge
	var rim: float = max(dx / SURROUND, dy_land / (SURROUND * 0.7))
	return lerp(g, hmin - 30.0, smoothstep(0.6, 0.95, rim))

## Archipelago: the reef strip is one shore of a small round island. Land rises to a hill
## inland; past the coast the seafloor falls to the deep. The edge of the strip blends in.
func _island_ground(x: float, y: float, base: float, w: int, d: int, hmin: float) -> float:
	var R: float = max(w, d) * 1.6
	var c := Vector2(w * 0.5, d * 0.55 + R * 0.55)
	var v := Vector2(x, y) - c
	# a wobbly coastline, not a circle
	var ang := atan2(v.y, v.x)
	var wob: float = 0.85 + 0.3 * _vnoise(cos(ang) * 2.0 + 7.0, sin(ang) * 2.0 + 3.0)
	var r: float = v.length() / (R * wob)
	var relief := (_vnoise(x * 0.02, y * 0.02) - 0.5) * 10.0
	var hill: float = lerp(hmin - 40.0, hmin + 70.0, smoothstep(1.15, 0.2, r)) + relief * smoothstep(1.2, 0.6, r)
	var dx: float = max(-x, x - w, 0.0)
	var dy: float = max(-y, y - d, 0.0)
	return lerp(base, hill, smoothstep(0.0, 70.0, max(dx, dy)))

## The other islands on the horizon: low-poly rock with a name and population label.
var far_islands := []
## World units per km of the archipelago's map (the nearest island sits ~800 units off).
const KM := 65.0

func _build_far_islands() -> void:
	for f in far_islands:
		f.queue_free()
	far_islands.clear()
	var isl: Array = arch_state.get("islands", [])
	var n := isl.size()
	var here: int = sim.focus()
	if here >= n:
		return
	# the island in view sits at its map place; the rest at theirs, KM world units per km
	var R: float = max(extent.x, extent.y) * 1.6
	var c0 := Vector3(extent.x * 0.5, 0.0, -(extent.y * 0.55 + R * 0.55))
	for k in n:
		if k == here:
			continue
		var off := Vector2(float(isl[k].x) - float(isl[here].x), float(isl[k].y) - float(isl[here].y)) * KM
		var node := Node3D.new()
		node.position = c0 + Vector3(off.x, 0.0, off.y)
		add_child(node)
		node.add_child(_island_mesh(k, max(int(isl[k].width), int(isl[k].depth)) * 6.0 * 1.4))
		var lab := Label3D.new()
		lab.billboard = BaseMaterial3D.BILLBOARD_ENABLED
		# one size on screen at any distance
		lab.fixed_size = true
		lab.pixel_size = 0.0012
		lab.font_size = 28
		lab.outline_size = 8
		lab.no_depth_test = true
		lab.modulate = Color(1.0, 0.9, 0.6)
		lab.position = Vector3(0, 230, 0)
		lab.name = "Label"
		node.add_child(lab)
		node.set_meta("island", k)
		far_islands.append(node)
	_label_far_islands()

func _label_far_islands() -> void:
	var isl: Array = arch_state.get("islands", [])
	for node in far_islands:
		var k: int = node.get_meta("island")
		if k < isl.size():
			var lab: Label3D = node.get_node("Label")
			var alive: PackedInt64Array = arch_state.get("alive", PackedInt64Array())
			var life: bool = k < alive.size() and alive[k] == 1
			lab.text = "%s\n%d plants, %d animals" % [isl[k].name, isl[k].plants, isl[k].animals]
			var web: bool = _web(isl[k])
			lab.modulate = Color(0.6, 1.0, 0.6) if web else (Color(1.0, 0.85, 0.55) if life else Color(0.8, 0.8, 0.8))

func _island_mesh(k: int, radius: float) -> MeshInstance3D:
	var n := 48
	var verts := PackedVector3Array()
	var idx := PackedInt32Array()
	var span := radius * 1.4
	for j in n + 1:
		for i in n + 1:
			var x: float = -span + 2.0 * span * i / n
			var z: float = -span + 2.0 * span * j / n
			var ang := atan2(z, x)
			var wob: float = 0.8 + 0.4 * _vnoise(cos(ang) * 2.0 + k * 11.0, sin(ang) * 2.0 + k * 5.0)
			var r: float = Vector2(x, z).length() / (radius * wob)
			var y: float = lerp(-45.0, 60.0 + 40.0 * _vnoise(k * 3.0, 1.0), smoothstep(1.1, 0.15, r)) + (_vnoise(x * 0.02 + k, z * 0.02) - 0.5) * 18.0 * smoothstep(1.1, 0.5, r)
			verts.append(Vector3(x, y, z))
	for j in n:
		for i in n:
			var a := j * (n + 1) + i
			var c := a + n + 1
			idx.append_array([a, c, a + 1, a + 1, c, c + 1])
	var norms := PackedVector3Array()
	norms.resize(verts.size())
	for j in n + 1:
		for i in n + 1:
			var ex: Vector3 = verts[j * (n + 1) + min(i + 1, n)] - verts[j * (n + 1) + max(i - 1, 0)]
			var ez: Vector3 = verts[min(j + 1, n) * (n + 1) + i] - verts[max(j - 1, 0) * (n + 1) + i]
			norms[j * (n + 1) + i] = ez.cross(ex).normalized()
	var arrays := []
	arrays.resize(Mesh.ARRAY_MAX)
	arrays[Mesh.ARRAY_VERTEX] = verts
	arrays[Mesh.ARRAY_NORMAL] = norms
	arrays[Mesh.ARRAY_INDEX] = idx
	var mesh := ArrayMesh.new()
	mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)
	var mi := MeshInstance3D.new()
	mi.mesh = mesh
	mi.material_override = rock_mat
	return mi

static func _hash(i: int, j: int) -> float:
	var n := (i * 374761393 + j * 668265263) & 0x7fffffff
	n = ((n ^ (n >> 13)) * 1274126177) & 0x7fffffff
	return float(n & 0xffff) / 65535.0

## Value noise 0..1.
static func _vnoise(x: float, y: float) -> float:
	var i := floori(x)
	var j := floori(y)
	var fx := x - i
	var fy := y - j
	fx = fx * fx * (3.0 - 2.0 * fx)
	fy = fy * fy * (3.0 - 2.0 * fy)
	var a: float = lerp(_hash(i, j), _hash(i + 1, j), fx)
	var b: float = lerp(_hash(i, j + 1), _hash(i + 1, j + 1), fx)
	return lerp(a, b, fy)

## Sea to the horizon: fine near the reef (the swell is vertex motion), coarsening outward.
func _build_water() -> void:
	var line := PackedFloat32Array()
	var t := 0.0
	var step := 8.0
	var pos := [0.0]
	while t < 9000.0:
		t += step
		pos.append(t)
		if t > 700.0:
			step = min(step * 1.25, 800.0)
	for k in range(pos.size() - 1, 0, -1):
		line.append(-pos[k])
	for v in pos:
		line.append(v)
	var n := line.size()
	var verts := PackedVector3Array()
	verts.resize(n * n)
	var norms := PackedVector3Array()
	norms.resize(n * n)
	for j in n:
		for i in n:
			verts[j * n + i] = Vector3(line[i], 0.0, line[j])
			norms[j * n + i] = Vector3.UP
	var idx := PackedInt32Array()
	for j in n - 1:
		for i in n - 1:
			var a := j * n + i
			var c := a + n
			idx.append_array([a, c, a + 1, a + 1, c, c + 1])
	var arrays := []
	arrays.resize(Mesh.ARRAY_MAX)
	arrays[Mesh.ARRAY_VERTEX] = verts
	arrays[Mesh.ARRAY_NORMAL] = norms
	arrays[Mesh.ARRAY_INDEX] = idx
	var pm := ArrayMesh.new()
	pm.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)
	water_mat = ShaderMaterial.new()
	water_mat.shader = load("res://shaders/water.gdshader")
	water = MeshInstance3D.new()
	water.mesh = pm
	water.material_override = water_mat
	water.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	water.position = Vector3(extent.x * 0.5, 0.0, -extent.y * 0.5)
	water.custom_aabb = AABB(Vector3(-9000, -10, -9000), Vector3(18000, 20, 18000))
	water_mat.set_shader_parameter("calm_center", Vector2(extent.x * 0.5, -extent.y * 0.5))
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
	# PASS: only the accepted left click is eaten; wheel and drags still reach the camera
	chart.mouse_filter = Control.MOUSE_FILTER_PASS
	chart.tooltip_text = "Click: population / traits"
	chart.gui_input.connect(func(e: InputEvent) -> void:
		if e is InputEventMouseButton and e.pressed and e.button_index == MOUSE_BUTTON_LEFT:
			chart_traits = not chart_traits
			chart.queue_redraw()
			chart.accept_event())
	chart.draw.connect(_draw_chart)
	layer.add_child(chart)

## Lever panel (garden only), under the goal banner: the body panel owns the left column.
func _build_levers() -> void:
	lever_panel = load("res://scripts/lever_panel.gd").new()
	lever_panel.setup("Between the islands (L to hide)" if arch else "Your world's environment (L to hide)", ARCH_LEVERS if arch else LEVERS, _apply_lever,
		func() -> Dictionary: return sim.levers() if ok and not sim.busy() else {},
		"The currents' strength is a real rate in the model, x1/4 to x4." if arch else "Each is a real rate in the model, x1/4 to x4. Run an experiment: change one, press T, read the report.")
	lever_panel.set_anchors_preset(Control.PRESET_CENTER_TOP)
	lever_panel.offset_left = -180
	lever_panel.offset_top = 124
	layer.add_child(lever_panel)
	_show_levers()

## False while the reef is on the worker (sliders are locked then, see `_lock_levers`).
func _apply_lever(key: String, mult: float) -> bool:
	if not ok or sim.busy():
		return false
	var was: float = sim.levers().get(key, 1.0)
	sim.set_lever(key, mult)
	var now: float = sim.levers().get(key, 1.0)
	if abs(now - was) > 0.001:
		_mark_lever(key, now)
	if campaign and garden:
		campaign.state["levers"] = sim.levers()
	if garden:
		wave_mult = sim.levers().get("waves", 1.0)
	return true

## One marker per lever per day: a slider dragged through several stops keeps only where it ended.
func _mark_lever(key: String, mult: float) -> void:
	var day := sim.t_days()
	var name: String = key
	for lv in LEVERS + ARCH_LEVERS:
		if lv[0] == key:
			name = lv[1].to_lower()
	var text := "%s x%s" % [name, ("%.2f" % mult).trim_suffix("0").trim_suffix("0").trim_suffix(".")]
	if not lever_marks.is_empty() and lever_marks[-1][2] == key and day - lever_marks[-1][0] < 1.0:
		lever_marks[-1] = [day, text, key]
	else:
		lever_marks.append([day, text, key])
	chart.queue_redraw()

## Set one lever and show it (capture flag --levers=key:mult).
func _set_lever(key: String, mult: float) -> void:
	_apply_lever(key, mult)
	_show_levers()

func _show_levers() -> void:
	if lever_panel == null or not ok or sim.busy():
		return
	if garden:
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
	if not (garden or arch) or lever_panel == null or not ok or sim.busy():
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
	if arch and ok and on and not sim.busy():
		if arch_state.is_empty():
			arch_state = sim.archipelago()
			var isl: Array = arch_state.get("islands", [])
			if sim.focus() < isl.size():
				wave_mult = float(isl[sim.focus()].waves)
		if far_islands.is_empty():
			_build_far_islands()
		if campaign:
			_arch_release()

## Archipelago: kept creatures not yet out here land on the Lagoon (island 0), each its own
## lineage. The view moves to the Lagoon to show them arrive.
func _arch_release() -> void:
	var landed := []
	# the six kept most recently: more lineages than that only crowd the Lagoon
	for k in campaign.state.kept.slice(-6):
		if arch_released.has(k.name):
			continue
		if sim.focus() != 0:
			_set_island(0)
		var c: int = sim.release(PackedByteArray(k.genome), PackedInt64Array(k.mass), RELEASE_SPORES)
		if c >= 0:
			arch_released[k.name] = true
			names[c] = k.name
			landed.append(k.name)
	if not landed.is_empty():
		campaign.toast.emit("On the Lagoon: " + ", ".join(PackedStringArray(landed)), "Spores that drift off the deep edge may reach the other islands (I to look).", Color(0.5, 0.85, 1.0))

## Show island k: its rock, its sea state, and a fresh chart (the history was the other island's).
func _set_island(k: int) -> void:
	if not sim.set_focus(k):
		return
	had_creature = false
	selected = -1
	organism = {}
	creatures = PackedInt64Array()
	rings_mm.instance_count = 0
	for id in skins:
		skins[id].queue_free()
	skins.clear()
	history = PackedFloat32Array()
	lever_marks.clear()
	# islands differ in size: the reef, its rock, the sea and the camera follow the new one
	extent = sim.extent()
	target = Vector3(extent.x * 0.5, 30.0, -extent.y * 0.35)
	water.position = Vector3(extent.x * 0.5, water.position.y, -extent.y * 0.5)
	water_mat.set_shader_parameter("calm_center", Vector2(extent.x * 0.5, -extent.y * 0.5))
	_build_terrain()
	arch_state = sim.archipelago()
	var isl: Array = arch_state.get("islands", [])
	if k < isl.size():
		wave_mult = float(isl[k].waves)
	_build_far_islands()
	timer = 0.0

## Discovery: one lineage split into two kinds across islands (archipelago.rs SPLIT_GOAL held
## SPLIT_HOLD days, checked daily in the sim). Once per save, a toast and a codex page.
func _check_two_kinds(a: Dictionary) -> void:
	if campaign == null or campaign.state.get("two_kinds", false):
		return
	if float(a.get("held", 0.0)) < float(a.get("hold", 150.0)):
		return
	var isl: Array = a.get("islands", [])
	var sa: int = a.get("split_a", -1)
	var sb: int = a.get("split_b", -1)
	if sa < 0 or sb < 0 or sa >= isl.size() or sb >= isl.size():
		return
	campaign.state["two_kinds"] = true
	var nm: String = names.get(int(a.split_lineage), "lineage #%d" % int(a.split_lineage))
	var page := "[b]Two kinds.[/b] %s's descendants on %s and on %s have grown apart: their genomes now differ by %.1f letters in 100 beyond the variety inside each island, and have for %d days. Isolation first, then difference: how new species begin." % [nm, isl[sa].name, isl[sb].name, float(a.divergence) * 100.0, int(a.get("hold", 150))]
	# a page of its own: field notes rotate out, a discovery stays
	campaign.state.codex.append({"title": "Discovery: two kinds", "text": page, "scale": 7})
	campaign.save()
	campaign.changed.emit()
	campaign.toast.emit("Discovery: two kinds", "%s on %s and %s have split apart. Saved in the codex (J)." % [nm, isl[sa].name, isl[sb].name], Color(0.6, 0.95, 1.0))

## A living web on island `e` (an `archipelago()` island entry): its size's plants, an animal.
static func _web(e: Dictionary) -> bool:
	return int(e.plants) >= int(e.get("need_plants", 1)) and int(e.animals) >= int(e.get("need_animals", 1))

func _island_name() -> String:
	var isl: Array = arch_state.get("islands", [])
	var k: int = sim.focus()
	return isl[k].name if k < isl.size() else "the islands"

## Archipelago HUD: the island in view, then every island as a dot (filled: alive).
func _arch_text() -> String:
	var isl: Array = arch_state.get("islands", [])
	if isl.is_empty():
		return ""
	var alive: PackedInt64Array = arch_state.get("alive", PackedInt64Array())
	var k: int = sim.focus()
	var t := ""
	if k < isl.size():
		var e: Dictionary = isl[k]
		t += "[color=#9ab]%dx%d tiles, waves x%s, UV x%s, food x%s[/color]\n" % [e.width, e.depth, _mult(e.waves), _mult(e.uv), _mult(e.food)]
	var dots := []
	for j in isl.size():
		# filled: a living web there (enough plants and animals)
		var on: bool = _web(isl[j])
		var nm: String = isl[j].name
		dots.append(("[color=#7f7]●[/color] " if on else "[color=#776]○[/color] ") + ("[b]%s[/b]" % nm if j == k else nm))
	t += "[font_size=13]" + "  ".join(PackedStringArray(dots)) + "[/font_size]\n"
	return t

static func _mult(v: float) -> String:
	return ("%.2f" % v).trim_suffix("0").trim_suffix("0").trim_suffix(".")

func next_island() -> void:
	if not arch or not ok or sim.busy():
		return
	var n: int = sim.archipelago().get("islands", []).size()
	if n > 0:
		_set_island((sim.focus() + 1) % n)

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
				if g.has("genus"):
					genera[c] = g.genus
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
			if k.has("species"):
				genera[c] = str(k.species).get_slice(" ", 0)
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
	search_from = -1.0
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
	if garden or arch:
		_lock_levers(false)
		_show_levers()
	if arch:
		# the goal reads this: never pair a stale split with the new day
		arch_state = sim.archipelago()
	# the report names the run's lineages: write it before any rebuild renames them
	var text := _ff_report(r)
	ff.show_report(text + "\n[color=#9ab]Any key to close. Saved in the codex (J).[/color]")
	timer = 0.0
	if campaign:
		campaign.field_note(ff_title, text)
	if garden:
		_release_kept()
		save_garden()
	# the recipe or sky changed while this world was on the worker: rebuild it now
	if is_stale() and rebuild_world():
		mark_built()
		campaign.toast.emit("Rebuilt from the world below", "Your recipe, sky or coast founders changed while this one ran ahead, so it started over.", Color(1.0, 0.8, 0.45))

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
		var rec := {"name": names[int(l.lineage)], "genome": Array(l.genome), "mass": Array(l.mass), "cells": l.cells}
		if genera.has(int(l.lineage)):
			rec["genus"] = genera[int(l.lineage)]
		out.append(rec)
	out.append_array(unplaced)
	campaign.state.garden = out
	campaign.state.saved_at = Time.get_unix_time_from_system()
	campaign.save()

## Keep the selected body: its genome goes to the player's world.
func keep_selected(any := false) -> void:
	if campaign == null or organism.is_empty() or sim.busy():
		return
	# before the reef goal is met, K is for creatures: keeping a small body by mistake looked
	# like progress and was not (Shift+K keeps any body)
	if not any and not garden and not campaign.is_done(6) and not organism.get("creature", false):
		var why := []
		if int(organism.cells) < 20:
			why.append("%d cells (needs 20+)" % organism.cells)
		if int(organism.jobs) < 3:
			why.append("%d job%s (needs 3)" % [organism.jobs, "" if int(organism.jobs) == 1 else "s"])
		if float(organism.get("speed", 0.0)) <= 0.0:
			why.append("not crawling")
		campaign.toast.emit("Not a creature yet", "This body has " + ", ".join(PackedStringArray(why)) + ". Shift+G finds a creature (gold ring); Shift+K keeps this one anyway.", Color(1.0, 0.55, 0.45))
		return
	var org := organism.duplicate()
	if garden and genera.has(int(organism.lineage)):
		org["genus"] = genera[int(organism.lineage)]
	var name: String = campaign.keep(org, "your world" if garden else where())
	if name == "":
		return
	var what := "a creature" if organism.creature else "%d cells, %d jobs" % [organism.cells, organism.jobs]
	campaign.toast.emit("Kept " + name + " (" + what + ")", "It lands on the islands of the archipelago, and later in your world. Keep several and see who survives.", Color(0.75, 1.0, 0.55))

## Where a body on this reef comes from, for kept creatures and the HUD.
func where() -> String:
	if garden:
		return "your world"
	if arch:
		return "the archipelago (%s)" % _island_name()
	if expedition > 0:
		return "the expedition of %s" % _date_text(expedition)
	return "reef run %d" % (campaign.state.run if campaign else 1)

static func _date_text(d: int) -> String:
	return "%04d-%02d-%02d" % [d / 10000, (d / 100) % 100, d % 100]

## Today's expedition: a reef seeded by the date, the same for everyone who plays today.
func start_expedition() -> void:
	if campaign == null or garden or arch or sim.busy():
		return
	# UTC: one day's reef for every player, wherever they are
	var t := Time.get_date_dict_from_system(true)
	var day: int = t.year * 10000 + t.month * 100 + t.day
	_restart(day)
	expedition = day
	var first: bool = int(campaign.state.get("expedition_day", 0)) != day
	campaign.state["expedition_day"] = day
	campaign.save()
	campaign.toast.emit("Expedition of %s" % _date_text(day), ("A new reef appears every day, the same for every player. " if first else "Back on today's reef, from its start. ") + "Find its creatures and keep them.", Color(0.5, 0.85, 1.0))

func _restart(run_seed: int) -> void:
	search_from = -1.0
	had_creature = false
	for id in skins:
		skins[id].queue_free()
	skins.clear()
	ok = sim.start(ProjectSettings.globalize_path(RATES), ProjectSettings.globalize_path(SEED), run_seed)
	selected = -1
	organism = {}
	history = PackedFloat32Array()
	lever_marks.clear()
	if ok:
		sim.step(1, DT)

## A fresh reef from the same founders with new dice: another try at a creature.
func new_run() -> void:
	if campaign and arch and not sim.busy():
		_new_archipelago()
		return
	if campaign == null or garden or arch or sim.busy():
		return
	var run: int = campaign.new_run()
	expedition = 0
	_restart(run)
	campaign.toast.emit("Run %d" % run, "A new reef from the same founders. Different dice, different history.", Color(1.0, 0.5, 0.65))

## Archipelago: the same islands with new dice, the kept creatures landing on the Lagoon again.
## Which island is reached first, and by whom, is luck: that luck is the founder effect.
func _new_archipelago() -> void:
	search_from = -1.0
	had_creature = false
	arch_seed += 1
	for id in skins:
		skins[id].queue_free()
	skins.clear()
	var currents: float = sim.levers().get("currents", 1.0)
	ok = sim.start_archipelago(ProjectSettings.globalize_path(RATES), ProjectSettings.globalize_path(SEED), arch_seed)
	if not ok:
		return
	sim.set_lever("currents", currents)
	selected = -1
	organism = {}
	names.clear()
	arch_released.clear()
	arch_state = {}
	history = PackedFloat32Array()
	lever_marks.clear()
	# a new archipelago starts on the Lagoon: its size, not the last island's
	extent = sim.extent()
	target = Vector3(extent.x * 0.5, 30.0, -extent.y * 0.35)
	water.position = Vector3(extent.x * 0.5, water.position.y, -extent.y * 0.5)
	water_mat.set_shader_parameter("calm_center", Vector2(extent.x * 0.5, -extent.y * 0.5))
	_build_terrain()
	sim.step(1, DT)
	arch_state = sim.archipelago()
	wave_mult = float(arch_state.islands[0].waves)
	_build_far_islands()
	_arch_release()
	_show_levers()

func _notification(what: int) -> void:
	if what == NOTIFICATION_WM_CLOSE_REQUEST and active:
		save_garden()

func explain_text() -> String:
	return GARDEN_EXPLAIN if garden else (ARCH_EXPLAIN if arch else EXPLAIN)

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
	_place_profile()
	if sim.busy():
		var r: Dictionary = sim.poll_fast_forward()
		if r.is_empty():
			ff.show_progress("%s   %d%%" % [ff_title, int(sim.fast_forward_progress() * 100.0)], sim.fast_forward_progress())
			return
		_ff_done(r)
	if search_from >= 0.0:
		_search_step()
	elif speed() > 0:
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
	_place_markers()
	timer -= delta
	if timer <= 0.0:
		timer = 0.5
		if garden:
			save_timer -= 0.5
			if save_timer <= 0.0:
				save_timer = 30.0
				save_garden()
		stats = sim.stats()
		if arch:
			arch_state = sim.archipelago()
			_label_far_islands()
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
		_announce_creature()
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
		mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		add_child(mi)
		skins[id] = mi
	if creature_mat == null:
		creature_mat = ShaderMaterial.new()
		creature_mat.shader = load("res://shaders/creature_skin.gdshader")
	skins[id].material_override = creature_mat if creatures.has(id) else skin_mat
	skins[id].mesh = am

## Gold markers over bodies that meet the creature goal right now (their skins glow too).
func _update_rings() -> void:
	creatures = sim.creatures()
	marker_at = PackedVector3Array()
	for id in creatures:
		var pts: PackedVector3Array = sim.body_points(id)
		if pts.is_empty():
			continue
		var c := Vector3.ZERO
		var top := -INF
		for p in pts:
			c += p
			top = max(top, p.y)
		c /= pts.size()
		marker_at.append(Vector3(c.x, top + 2.5, c.z))
	rings_mm.instance_count = marker_at.size()
	_place_markers()

## One size on screen at any distance: scale with the camera's distance to each marker.
func _place_markers() -> void:
	if cam == null:
		return
	for i in marker_at.size():
		var s: float = clamp(cam.global_position.distance_to(marker_at[i]) * 0.06, 1.5, 60.0)
		rings_mm.set_instance_transform(i, Transform3D(Basis().scaled(Vector3(s, s, s)), marker_at[i] + Vector3(0, s * 0.5, 0)))

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

## Shift+G: run the reef fast, checking each frame, until a creature appears.
func start_search() -> void:
	if not ok or sim.busy() or garden:
		return
	if not creatures.is_empty():
		next_creature()
		return
	search_from = 0.0
	campaign_toast("Searching for a creature", "The reef runs fast until one appears (Shift+G again to stop).", Color(1.0, 0.82, 0.4))

func _search_step() -> void:
	# a day a frame, checked at noon: jobs come from light-switched genes, so creatures are a
	# midday thing and a check at night would skip them
	var t: float = sim.t_days()
	var n: int = max(1, int(round((floor(t - 0.5) + 1.5 - t) / DT)))
	sim.step(n, DT)
	search_from += n * DT
	if not sim.creatures().is_empty():
		search_from = -1.0
		_update_rings()
		speed_idx = 0
		had_creature = true
		next_creature()
		campaign_toast("Found one", "Paused on it. K keeps it; space plays on.", Color(1.0, 0.82, 0.4))
	elif search_from > SEARCH_DAYS:
		search_from = -1.0
		campaign_toast("No creature in %d days" % int(SEARCH_DAYS), "This run may not make one soon: N starts a new run with different dice.", Color(0.8, 0.85, 0.95))

## A creature appeared on its own: stop the clock on it once, so it is not missed.
func _announce_creature() -> void:
	var now := not creatures.is_empty()
	if now and not had_creature and not garden and speed_idx > 0 and campaign and not campaign.is_done(6):
		speed_idx = 0
		next_creature()
		campaign_toast("A creature!", "Gold ring: a crawling body with 3+ jobs. Paused on it: K keeps it, space plays on.", Color(1.0, 0.82, 0.4))
	had_creature = now

func campaign_toast(title: String, body: String, col: Color) -> void:
	if campaign:
		campaign.toast.emit(title, body, col)

## Fly to the next creature and select it.
func next_creature() -> void:
	if creatures.is_empty():
		if campaign:
			campaign.toast.emit("No creature right now", "They come and go, mostly around midday when light switches cells' jobs. Shift+G searches: the reef runs fast and stops on the next one.", Color(0.8, 0.85, 0.95))
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

## The body panel goes under the HUD's left panel, whose height changes with its text.
func _place_profile() -> void:
	var hud = get_tree().current_scene.get("hud") if get_tree().current_scene else null
	var lp = hud.get("left_panel") if hud else null
	if lp is Control and lp.is_visible_in_tree():
		profile.position.y = min(lp.get_global_rect().end.y + 10.0, get_viewport().get_visible_rect().size.y - profile.size.y - 8.0)
	else:
		profile.position.y = 12.0

func _update_camera() -> void:
	pitch = clamp(pitch, -1.45, 0.25)
	dist = clamp(dist, 6.0, 900.0)
	var off := Vector3(sin(yaw) * cos(pitch), -sin(pitch), cos(yaw) * cos(pitch)) * dist
	var p := target + off
	if not ground.is_empty():
		# never under the ground, reef rock included
		var gy: float = _ground(p.x, -p.z, ground[0], ground[1], ground[2], ground[3], ground[4])
		p.y = max(p.y, gy + 4.0)
	cam.position = p
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
					keep_selected(e.shift_pressed)
			KEY_N:
				if not e.echo:
					if e.shift_pressed:
						start_expedition()
					else:
						new_run()
			KEY_G:
				if not e.echo:
					if e.shift_pressed:
						if search_from >= 0.0:
							search_from = -1.0
							campaign_toast("Search stopped", "", Color(0.8, 0.85, 0.95))
						else:
							start_search()
					else:
						next_creature()
			KEY_S:
				if not e.echo:
					skin_all = not skin_all
					_update_skins()
			KEY_T:
				if not e.echo:
					var days: float = FF_DAYS * (5.0 if e.shift_pressed else 1.0)
					start_ff(days, "Fast-forward: %d days%s" % [int(days), _lever_note()])
			KEY_I:
				if not e.echo:
					next_island()
			KEY_L:
				if (garden or arch) and not e.echo:
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
	if arch:
		var a: Dictionary = arch_state
		if a.is_empty():
			return {}
		var isl: Array = a.get("islands", [])
		var n := isl.size()
		var got := 0
		var short := []
		for k in n:
			var e: Dictionary = isl[k]
			if _web(e):
				got += 1
			else:
				short.append("%s (%d plants, %d animals)" % [e.name, e.plants, e.animals])
		_check_two_kinds(a)
		var now := "a living web on %d of %d islands" % [got, n]
		if not short.is_empty() and short.size() <= 2:
			now += "; short: " + ", ".join(PackedStringArray(short))
		var done: bool = float(a.get("web_day", -1.0)) >= 0.0
		var held: float = a.get("web_held", 0.0)
		var hold: float = a.get("web_hold", 100.0)
		if got == n and held > 0.0:
			now += ", held %d of %d days" % [int(held), int(hold)]
		var p: float = 0.6 * float(got) / max(n, 1) + 0.4 * clamp(held / hold, 0.0, 1.0)
		return {"p": 1.0 if done else p, "done": done, "args": [n, int(hold)], "now": now, "who": "the islands' plants and animals"}
	if garden:
		var who: String = names.get(int(g.won_lineage), "lineage %d" % g.won_lineage)
		var lead: String = names.get(int(g.best_lineage), "none yet")
		return {"p": min(1.0, float(g.best_days) / g.need_days), "done": g.won_lineage >= 0, "args": [g.need_days, int(g.window_days)],
			"now": "best: %s, creatures on %d of the last %d days" % [lead, g.best_days, int(g.window_days)], "who": who + " in your world"}
	var body: float = 0.5 * min(1.0, float(near_c) / g.min_cells) + 0.5 * min(1.0, float(near_j) / g.min_jobs)
	var now := "best crawler %d cells, %d jobs" % [near_c, near_j]
	if not organism.is_empty() and organism.get("creature", false) and campaign and not campaign.is_done(6):
		now = "[color=#ffd166][b]Press K to keep this creature[/b][/color]" + ("  (paused: space plays on)" if speed_idx == 0 else "")
	elif g.qualifying > 0:
		now = "%d creature(s) on the reef now: G to find one, K to keep it" % g.qualifying
	return {"p": 0.9 * body, "done": false, "args": [g.min_cells, g.min_jobs], "now": now}

func hud_text() -> String:
	if not ok:
		return "[color=#f88]%sreef_seed.json or rates0.json missing: run primordium reef-seed[/color]" % ElementsSim.data_dir()
	if stats.is_empty():
		return ""
	var title: String = "Your world" if garden else ("Archipelago: " + _island_name() if arch else ("Expedition " + _date_text(expedition) if expedition > 0 else "Reef, run %d" % (campaign.state.run if campaign else 1)))
	var t := "[b][font_size=20]%s[/font_size][/b]  [color=#9ab]day %d[/color]\n" % [title, int(env.get("t_days", 0.0))]
	# the four numbers worth a glance, as tiles
	t += "[table=4]"
	for c in [["cells", stats.cells], ["bodies", stats.bodies], ["biggest", stats.biggest], ["lineages", stats.lineages]]:
		t += "[cell padding=0,0,14,0][center][font_size=22][b]%s[/b][/font_size]\n[color=#8ab][font_size=12]%s[/font_size][/color][/center][/cell]" % [_short(c[1]), c[0]]
	t += "[/table]\n"
	if not garden:
		var fs: Dictionary = ReefSim.founder_source()
		if fs.from_coast:
			t += "[color=#ffd080]founded by %d cells from your coast[/color]\n" % fs.count
	var now := _now_line()
	if now != "":
		t += "[color=#dde]%s[/color]\n" % now
	if arch:
		t += _arch_text()
	# what the colours mean, as swatches, while they apply
	if by_lineage:
		t += "[color=#9ab]colour: lineage (C for jobs)[/color]\n"
	else:
		t += "[font_size=12][color=#40ccd9]■[/color] copy [color=#59cc4d]■[/color] feed [color=#f2e0b8]■[/color] skin [color=#bf3852]■[/color] shade [color=#ffbf33]■[/color] glue  [color=#789]C lineage[/color][/font_size]\n"
	var parts := []
	for v in stats.top_lineages.slice(0, 3):
		parts.append("%s %.0f%%" % [names.get(int(v.x), "#%d" % int(v.x)), v.y * 100.0])
	if not parts.is_empty():
		t += "[color=#9ab]largest: %s[/color]\n" % ", ".join(PackedStringArray(parts))
	if garden and stats.cells == 0:
		t += "[color=#f9c]Nothing alive here. Keep creatures at the reef (K) to release them.[/color]\n"
	if arch and stats.cells == 0:
		if campaign and campaign.state.kept.is_empty():
			t += "[color=#f9c]Nothing alive here. Keep a creature at the reef (K) and it lands on the Lagoon.[/color]\n"
		elif sim.focus() > 0:
			t += "[color=#f9c]Nothing here yet: spores have to drift in from another island (stronger currents, L, carry more).[/color]\n"
	if stats.unsure > 0:
		t += "[color=#f9c]%d cells where the coast's table is thin[/color]\n" % stats.unsure
	# the rest on demand (hud.gd: D details, ? keys)
	t += "[details]"
	t += "crawling %d bodies (%d many-celled)   tallest %d   generation %d\nadrift %d   spores %d   tears %d   bites %d\nspore bit error %.2f%%\n" % [stats.crawlers, stats.crawlers_multi, stats.tallest, stats.generation, stats.adrift, stats.spores, stats.tears, stats.bites, stats.bit_error * 100.0]
	if not garden and not arch:
		t += "%d of %d founders' lineages alive\n" % [stats.lineages, stats.founders]
	t += "[color=#9ab]glowing on the surface: spores and torn pieces adrift; green rim: a crawling body; red flash: a bite[/color]\n"
	if not stats.discoveries.is_empty():
		t += "[b]Discoveries[/b]\n"
		for d in stats.discoveries:
			t += "[color=#cbf]" + d + "[/color]\n"
	t += "[/details]"
	t += "[keys]Tab: switch scale, J codex\nright-drag orbit, shift/middle-drag pan, wheel zoom\nleft-click a body, F fly to it, B biggest, Esc clear\nG next creature (gold ring), Shift+G search for one, K keep the selected body (Shift+K any body)%s\nS smooth skins on every body\nT fast-forward 100 days (Shift+T 500) with a report%s\nO auto-orbit, C lineage colours, space pause, 1-5 speed[/keys]" % [", N new archipelago" if arch else ("" if garden else ", N new run\nShift+N today's expedition (the same reef for everyone today)"), "\nL environment levers" if garden else ("\nI next island, L currents" if arch else "")]
	return t

## The one thing worth saying about the reef right now.
func _now_line() -> String:
	if search_from >= 0.0:
		return "[color=#ffd166]searching for a creature... %d days so far[/color]" % int(search_from)
	if creatures.size() > 0 and not garden:
		return "[color=#ffd166]%d creature%s here now: G to find one, K to keep it[/color]" % [creatures.size(), "" if creatures.size() == 1 else "s"]
	if not garden and not arch and campaign and not campaign.is_done(6):
		return "no creature right now: Shift+G searches for one"
	if stats.crawlers_multi > 0:
		return "%d many-celled bodies are crawling" % stats.crawlers_multi
	if stats.multicellular > 0:
		return "%d bodies have grown past one cell" % stats.multicellular
	if stats.cells > 0:
		return "single cells settling on the rock"
	return ""

static func _short(n: int) -> String:
	if n >= 9950:
		return "%.0fk" % (n / 1000.0)
	if n >= 1000:
		return "%.1fk" % (n / 1000.0)
	return str(n)

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
	var title := "Reef over time: cells (white), mean body size (gold), tallest body (cyan), lineages alive (violet)"
	if chart_traits:
		title = "What its cells do, on average: polymerase, metabolism, membrane, pigment, glue"
	chart.draw_string(font, Vector2(8, 14), title, HORIZONTAL_ALIGNMENT_LEFT, sz.x - 108, 11, Color(1, 1, 1, 0.75))
	chart.draw_string(font, Vector2(sz.x - 98, 14), "click: " + ("population" if chart_traits else "traits"), HORIZONTAL_ALIGNMENT_RIGHT, 90, 10, Color(0.6, 0.75, 0.85))
	var rows := history.size() / 11
	if rows < 2:
		return
	# traits leave room on the right for their current values
	var area := Rect2(8, 22, sz.x - (60 if chart_traits else 16), sz.y - 30)
	var t0: float = history[0]
	var t1: float = history[(rows - 1) * 11]
	for m in lever_marks:
		if t1 > t0 and m[0] >= t0 and m[0] <= t1:
			var x: float = area.position.x + area.size.x * (m[0] - t0) / (t1 - t0)
			chart.draw_dashed_line(Vector2(x, area.position.y), Vector2(x, area.end.y), Color(1, 1, 1, 0.45), 1.0, 3.0)
			chart.draw_string(font, Vector2(x + 3, area.end.y - 2), m[1], HORIZONTAL_ALIGNMENT_LEFT, -1, 10, Color(1, 1, 1, 0.7))
	var series := [[1, Color(1, 1, 1, 0.85)], [3, Color(1.0, 0.75, 0.2, 0.9)], [4, Color(0.4, 0.9, 1.0, 0.9)], [10, Color(0.8, 0.5, 1.0, 0.9)]]
	# traits share one range (empty days skipped), so their sizes compare; zoomed to it, since
	# selection moves the means by hundredths
	var lo := 0.0
	var top_t := 0.0
	var labels := []
	if chart_traits:
		var tc := [ROLE_COLORS[0], ROLE_COLORS[1], ROLE_COLORS[2], ROLE_COLORS[3], ROLE_COLORS[5]]
		series = []
		lo = INF
		top_t = -INF
		for k in 5:
			series.append([5 + k, tc[k]])
			for i in rows:
				if history[i * 11 + 1] > 0.0:
					lo = min(lo, history[i * 11 + 5 + k])
					top_t = max(top_t, history[i * 11 + 5 + k])
		if lo > top_t:
			return
		var pad: float = max((top_t - lo) * 0.08, 0.005)
		lo -= pad
		top_t += pad
		chart.draw_string(font, Vector2(area.end.x + 6, area.position.y + 8), "%.2f" % top_t, HORIZONTAL_ALIGNMENT_LEFT, -1, 9, Color(1, 1, 1, 0.45))
		chart.draw_string(font, Vector2(area.end.x + 6, area.end.y), "%.2f" % lo, HORIZONTAL_ALIGNMENT_LEFT, -1, 9, Color(1, 1, 1, 0.45))
	for s in series:
		var k: int = s[0]
		var top := 1.0
		if not chart_traits:
			for i in rows:
				top = max(top, history[i * 11 + k])
		var pts := PackedVector2Array()
		for i in rows:
			if chart_traits and history[i * 11 + 1] <= 0.0:
				continue
			var x: float = area.position.x + area.size.x * ((history[i * 11] - t0) / (t1 - t0) if t1 > t0 else float(i) / (rows - 1))
			var v: float = history[i * 11 + k]
			if chart_traits:
				# 5-day mean: light-switched genes make single days jump
				var sum := 0.0
				var cnt := 0
				for j in range(max(0, i - 4), i + 1):
					if history[j * 11 + 1] > 0.0:
						sum += history[j * 11 + k]
						cnt += 1
				v = sum / cnt
			var y: float = area.position.y + area.size.y * (1.0 - ((v - lo) / (top_t - lo) if chart_traits else v / top))
			pts.append(Vector2(x, y))
		if pts.size() > 1:
			chart.draw_polyline(pts, s[1], 1.5, true)
		if chart_traits and not pts.is_empty():
			var last := rows - 1
			while last > 0 and history[last * 11 + 1] <= 0.0:
				last -= 1
			labels.append([pts[-1].y, history[last * 11 + k], s[1]])
	# current values at the right end, pushed apart so they never overlap
	labels.sort_custom(func(a, b): return a[0] < b[0])
	var ys := []
	var last_y := -INF
	for l in labels:
		last_y = max(l[0] + 4, last_y + 10)
		ys.append(last_y)
	# pushed past the bottom: slide the whole stack back up
	var over: float = ys.back() - area.end.y if not ys.is_empty() else 0.0
	for i in labels.size():
		chart.draw_string(font, Vector2(area.end.x + 6, ys[i] - max(over, 0.0)), "%.3f" % labels[i][1], HORIZONTAL_ALIGNMENT_LEFT, -1, 10, labels[i][2])

## The recipe changed: this world starts again in the new sea. The reef runs again, the
## archipelago starts over with the kept creatures, your world comes back from its save.
## The reef and the archipelago start from the coast's founders; your world from kept creatures.
func uses_founders() -> bool:
	return not garden

## World versions this view was built from (main.gd sets them, campaign bumps them).
func is_stale() -> bool:
	if campaign == null:
		return false
	return get_meta("built", 0) != campaign.recipe_version() or (uses_founders() and get_meta("built_f", 0) != campaign.founders_version())

func mark_built() -> void:
	if campaign:
		set_meta("built", campaign.recipe_version())
		set_meta("built_f", campaign.founders_version())

func rebuild_world() -> bool:
	if not ok or sim.busy():
		return false
	if arch:
		arch_seed -= 1
		_new_archipelago()
	elif garden:
		if restored:
			save_garden()
		ok = sim.start_garden(ProjectSettings.globalize_path(RATES), ProjectSettings.globalize_path(SEED), 7)
		names.clear()
		mine.clear()
		genera.clear()
		unplaced.clear()
		restored = false
		history = PackedFloat32Array()
		lever_marks.clear()
		for id in skins:
			skins[id].queue_free()
		skins.clear()
		selected = -1
		organism = {}
		if ok:
			sim.step(1, DT)
			if active and campaign:
				_populate()
	else:
		expedition = 0
		_restart(campaign.state.run if campaign else 1)
	return true
