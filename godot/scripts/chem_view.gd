## Scale 0 view: the tidal-pool lattice. Pure renderer over ChemSim (Rust).
extends Node2D

const CELL := 8.0
const SPEEDS := [0, 1, 4, 16, 64]
const PALETTE := [
	Color(0, 0, 0),
	Color(0.92, 0.94, 1.00), # Hy
	Color(0.85, 0.45, 0.95), # Ne (noble)
	Color(0.95, 0.55, 0.25), # Li
	Color(0.45, 0.85, 0.45), # Bo
	Color(0.35, 0.35, 0.40), # Ka backbone
	Color(0.95, 0.30, 0.30), # Ox
	Color(0.30, 0.75, 0.95), # Fl
	Color(0.75, 0.55, 0.95), # Ar (noble)
]

var sim: ChemSim
var world: Node2D
var beach: ColorRect
var bonds: Node2D
var mm: MultiMesh
var atoms_node: MultiMeshInstance2D
var cam: Camera2D
var speed_idx := 2
var env := {}
var census := {}
var selected := Vector2i(-1, -1)
var census_timer := 0.0
var dragging := false
var bands: Array = []
var hybrid := {}
var discoveries: Array = []
var overlay: Node2D
var auto := true
## Zoom (px per cell) above which on-screen bands pin to detail: you see real atoms where you look.
const PIN_ZOOM := 14.0

func _ready() -> void:
	sim = ChemSim.new()
	add_child(sim)
	var grid: Vector2i = sim.grid_size()

	world = Node2D.new()
	world.scale = Vector2(CELL, CELL)
	add_child(world)

	beach = ColorRect.new()
	beach.size = Vector2(grid)
	var bm := ShaderMaterial.new()
	bm.shader = load("res://shaders/beach.gdshader")
	bm.set_shader_parameter("grid", Vector2(grid))
	beach.material = bm
	beach.mouse_filter = Control.MOUSE_FILTER_IGNORE
	world.add_child(beach)

	bonds = Node2D.new()
	bonds.draw.connect(_draw_bonds)
	world.add_child(bonds)

	mm = MultiMesh.new()
	mm.transform_format = MultiMesh.TRANSFORM_2D
	mm.use_colors = true
	mm.use_custom_data = true
	var quad := QuadMesh.new()
	quad.size = Vector2(1.0, 1.0)
	mm.mesh = quad
	mm.instance_count = sim.atom_count()
	atoms_node = MultiMeshInstance2D.new()
	atoms_node.multimesh = mm
	var am := ShaderMaterial.new()
	am.shader = load("res://shaders/atom.gdshader")
	am.set_shader_parameter("palette", PALETTE)
	atoms_node.material = am
	world.add_child(atoms_node)

	cam = Camera2D.new()
	add_child(cam)
	cam.position = Vector2(grid) * CELL * 0.5
	var vs := get_viewport_rect().size
	var fit: float = min(vs.x / (grid.x * CELL), vs.y / (grid.y * CELL)) * 0.95
	cam.zoom = Vector2(fit, fit)
	cam.make_current()

	var layer := CanvasLayer.new()
	layer.layer = 0
	add_child(layer)
	overlay = Node2D.new()
	overlay.draw.connect(_draw_overlay)
	layer.add_child(overlay)

	var rates := ProjectSettings.globalize_path("res://../data/rates0.json")
	if sim.load_rates(rates):
		sim.set_auto(auto)
	_refresh()

func px_per_cell() -> float:
	return CELL * cam.zoom.x

func speed() -> int:
	return SPEEDS[speed_idx]

func _process(delta: float) -> void:
	if speed() > 0:
		sim.step(speed())
	_refresh()
	census_timer -= delta
	if census_timer <= 0.0:
		census = sim.census()
		census_timer = 0.5
		_update_pins()
		hybrid = sim.hybrid_stats()
		discoveries = sim.discoveries()

func _refresh() -> void:
	env = sim.env()
	mm.buffer = sim.instance_buffer()
	var light: float = clamp(env.visible * 1.6 + 0.12, 0.0, 1.0)
	var am: ShaderMaterial = atoms_node.material
	am.set_shader_parameter("px_per_cell", px_per_cell())
	am.set_shader_parameter("light", light)
	var bm: ShaderMaterial = beach.material
	bm.set_shader_parameter("tide_level", env.tide_level)
	bm.set_shader_parameter("light", light)
	bm.set_shader_parameter("uv_glow", clamp(env.flare * 0.4, 0.0, 0.5))
	bonds.queue_redraw()
	bands = sim.bands()
	overlay.queue_redraw()

func world_to_screen(p: Vector2) -> Vector2:
	return (p * CELL - cam.position) * cam.zoom + get_viewport_rect().size * 0.5

func _update_pins() -> void:
	var vs := get_viewport_rect().size
	var left := screen_to_cell(Vector2.ZERO).x
	var right := screen_to_cell(vs).x
	for i in bands.size():
		var b: Dictionary = bands[i]
		var visible: bool = b.x1 > left and b.x0 <= right
		sim.pin_band(i, visible and px_per_cell() >= PIN_ZOOM)

func _draw_overlay() -> void:
	var grid: Vector2i = sim.grid_size()
	var font := ThemeDB.fallback_font
	for b in bands:
		var a := world_to_screen(Vector2(b.x0, 0))
		var z := world_to_screen(Vector2(b.x1, grid.y))
		var r := Rect2(a, z - a)
		if b.surrogate:
			overlay.draw_rect(r, Color(0.55, 0.35, 0.95, 0.16))
			overlay.draw_rect(r, Color(0.7, 0.55, 1.0, 0.5), false, 1.5)
		elif b.pinned:
			overlay.draw_rect(r, Color(1.0, 0.9, 0.4, 0.35), false, 1.0)
		var tag := ("SURROGATE" if b.surrogate else ("DETAIL (zoom)" if b.pinned else "DETAIL")) + "\nconf %.2f\n%d bonds" % [b.confidence, b.bonds]
		var y: float = max(a.y, 0.0) + 300.0
		overlay.draw_multiline_string(font, Vector2(a.x + 6, y), tag, HORIZONTAL_ALIGNMENT_LEFT, r.size.x - 8, 12, 3, Color(1, 1, 1, 0.75))
		if b.surrogate and b.has("pairs"):
			_draw_pair_bars(b.pairs, Rect2(a.x + 6, y + 48, r.size.x - 12, 260))

## Top bond types as bars, two-tone by element colour. The band's whole state, drawn.
func _draw_pair_bars(pairs: Array, area: Rect2) -> void:
	var sorted := pairs.duplicate()
	sorted.sort_custom(func(p, q): return p.n > q.n)
	var top: int = sorted[0].n if sorted.size() > 0 else 0
	if top <= 0:
		return
	var rows: int = min(10, sorted.size())
	var h: float = area.size.y / rows
	var font := ThemeDB.fallback_font
	for k in rows:
		var p: Dictionary = sorted[k]
		if p.n <= 0:
			break
		var w: float = (area.size.x - 4) * float(p.n) / float(top)
		var y := area.position.y + k * h
		overlay.draw_rect(Rect2(area.position.x, y + 14, w * 0.5, h - 18), PALETTE[p.za])
		overlay.draw_rect(Rect2(area.position.x + w * 0.5, y + 14, w * 0.5, h - 18), PALETTE[p.zb])
		overlay.draw_string(font, Vector2(area.position.x, y + 12), "%s %d" % [p.name, p.n], HORIZONTAL_ALIGNMENT_LEFT, -1, 10, Color(1, 1, 1, 0.7))

func _draw_bonds() -> void:
	var detail: float = smoothstep(18.0, 48.0, px_per_cell())
	var lines: PackedVector2Array = sim.bond_lines()
	if lines.size() > 0:
		bonds.draw_multiline(lines, Color(0.96, 0.92, 0.80, 0.85 - 0.35 * detail), lerp(0.16, 0.08, detail))
	var weak: PackedVector2Array = sim.weak_lines()
	if weak.size() > 0:
		bonds.draw_multiline(weak, Color(0.45, 0.85, 1.0, 0.30), 0.05)
	if selected.x >= 0:
		bonds.draw_rect(Rect2(Vector2(selected), Vector2.ONE), Color(1, 1, 0.3, 0.9), false, 0.08)

func screen_to_cell(sp: Vector2) -> Vector2i:
	var vs := get_viewport_rect().size
	var wp := cam.position + (sp - vs * 0.5) / cam.zoom
	return Vector2i(floor(wp.x / CELL), floor(wp.y / CELL))

func zoom_at(factor: float, sp: Vector2) -> void:
	var vs := get_viewport_rect().size
	var before := cam.position + (sp - vs * 0.5) / cam.zoom
	var z: float = clamp(cam.zoom.x * factor, 0.25, 20.0)
	cam.zoom = Vector2(z, z)
	cam.position = before - (sp - vs * 0.5) / cam.zoom

func focus(cell: Vector2, zoom: float) -> void:
	cam.zoom = Vector2(zoom, zoom)
	cam.position = cell * CELL

func selected_info() -> Dictionary:
	if selected.x < 0:
		return {}
	return sim.atom_info(selected.x, selected.y)

func _unhandled_input(e: InputEvent) -> void:
	if e is InputEventMouseButton:
		if e.button_index == MOUSE_BUTTON_WHEEL_UP and e.pressed:
			zoom_at(1.15, e.position)
		elif e.button_index == MOUSE_BUTTON_WHEEL_DOWN and e.pressed:
			zoom_at(1.0 / 1.15, e.position)
		elif e.button_index in [MOUSE_BUTTON_RIGHT, MOUSE_BUTTON_MIDDLE]:
			dragging = e.pressed
		elif e.button_index == MOUSE_BUTTON_LEFT and e.pressed:
			selected = screen_to_cell(e.position)
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
			KEY_R:
				sim.reset(randi() % 100000, sim.grid_size().x, sim.grid_size().y)
				mm.instance_count = sim.atom_count()
