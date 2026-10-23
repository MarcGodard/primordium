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
			KEY_R:
				sim.reset(randi() % 100000, sim.grid_size().x, sim.grid_size().y)
				mm.instance_count = sim.atom_count()
