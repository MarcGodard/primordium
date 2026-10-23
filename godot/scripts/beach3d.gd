## Scale 3 in 3D: the beach strip as terrain (sea on the left, dune on the right, the shore
## running away from the camera), the tide as a real sea level, cells as translucent
## membranes with a genome core, sized by their membrane and tinted by their genes, gold bridges
## between colony mates, red flashes where a cell was engulfed. Positions are the 2D view's
## (patch band across, y along the shore), so both views show the same cells.
extends Node3D

## World units of rise per unit of elevation (sea floor 0 .. dune 1).
const RISE := 120.0
const ELEV := Vector2(0.28, 0.77)

var width := 220.0
var length := 140.0
var rig: Node3D
var cam: Camera3D
var water: MeshInstance3D
var water_mat: ShaderMaterial
var rock_mat: ShaderMaterial
var mm: MultiMesh
var bridges: MeshInstance3D
var bridge_mesh: ImmediateMesh
var flashes: MultiMesh
var target := Vector3(110, 30, -70)
var yaw := 0.35
var pitch := -0.72
var dist := 140.0
var orbiting := false
var panning := false

func setup(w: float, l: float) -> void:
	width = w
	length = l
	target = Vector3(width * 0.45, 20.0, -length * 0.5)
	rig = load("res://scripts/sky_rig.gd").new()
	add_child(rig)
	_terrain()
	var pm := PlaneMesh.new()
	pm.size = Vector2(1400, 1400)
	pm.subdivide_width = 120
	pm.subdivide_depth = 120
	water_mat = ShaderMaterial.new()
	water_mat.shader = load("res://shaders/water.gdshader")
	water = MeshInstance3D.new()
	water.mesh = pm
	water.material_override = water_mat
	water.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	water.position = Vector3(width * 0.5, 0, -length * 0.5)
	add_child(water)
	var sphere := SphereMesh.new()
	sphere.radius = 1.0
	sphere.height = 2.0
	sphere.radial_segments = 20
	sphere.rings = 10
	var mat := ShaderMaterial.new()
	mat.shader = load("res://shaders/membrane.gdshader")
	mm = MultiMesh.new()
	mm.transform_format = MultiMesh.TRANSFORM_3D
	mm.use_colors = true
	mm.use_custom_data = true
	mm.mesh = sphere
	var mi := MultiMeshInstance3D.new()
	mi.multimesh = mm
	mi.material_override = mat
	add_child(mi)
	bridge_mesh = ImmediateMesh.new()
	bridges = MeshInstance3D.new()
	bridges.mesh = bridge_mesh
	var bm := StandardMaterial3D.new()
	bm.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	bm.albedo_color = Color(1.0, 0.82, 0.35)
	bm.emission_enabled = true
	bm.emission = Color(1.0, 0.75, 0.3)
	bm.emission_energy_multiplier = 2.0
	bridges.material_override = bm
	add_child(bridges)
	var spark := SphereMesh.new()
	spark.radius = 1.0
	spark.height = 2.0
	var sm := StandardMaterial3D.new()
	sm.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	sm.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	sm.vertex_color_use_as_albedo = true
	flashes = MultiMesh.new()
	flashes.transform_format = MultiMesh.TRANSFORM_3D
	flashes.use_colors = true
	flashes.mesh = spark
	var fi := MultiMeshInstance3D.new()
	fi.multimesh = flashes
	fi.material_override = sm
	add_child(fi)
	cam = Camera3D.new()
	cam.fov = 50.0
	cam.far = 4000.0
	add_child(cam)

func elevation(x: float) -> float:
	return ELEV.x + (ELEV.y - ELEV.x) * clamp(x / width, 0.0, 1.0)

## Ground height at (x along the slope, z along the shore): the beach's linear rise plus ripples.
func ground(x: float, z: float) -> float:
	var e := elevation(x)
	return (e - ELEV.x) * RISE + 1.2 * sin(x * 0.21 + z * 0.05) * sin(z * 0.13) + 0.8 * sin(z * 0.07 - x * 0.03)

func _terrain() -> void:
	var nx := 110
	var nz := 70
	var verts := PackedVector3Array()
	var idx := PackedInt32Array()
	for j in nz + 1:
		for i in nx + 1:
			var x := width * i / nx
			var z := length * j / nz
			verts.append(Vector3(x, ground(x, z), -z))
	for j in nz:
		for i in nx:
			var a := j * (nx + 1) + i
			idx.append_array([a, a + nx + 1, a + 1, a + 1, a + nx + 1, a + nx + 2])
	var st := SurfaceTool.new()
	st.begin(Mesh.PRIMITIVE_TRIANGLES)
	for k in idx:
		st.add_vertex(verts[k])
	st.generate_normals()
	rock_mat = ShaderMaterial.new()
	rock_mat.shader = load("res://shaders/rock.gdshader")
	var mi := MeshInstance3D.new()
	mi.mesh = st.commit()
	mi.material_override = rock_mat
	add_child(mi)
	# the sea floor beyond the beach
	var apron := PlaneMesh.new()
	apron.size = Vector2(3000, 3000)
	var ai := MeshInstance3D.new()
	ai.mesh = apron
	ai.position = Vector3(width * 0.5, -2.0, -length * 0.5)
	ai.material_override = rock_mat
	add_child(ai)

func set_active(on: bool) -> void:
	visible = on
	rig.set_active(on)
	if on:
		cam.make_current()
	else:
		cam.clear_current(false)

## A cell's centre: the 2D view's position, standing on the sand (or the sea floor).
func cell_pos(p2: Vector2, r: float) -> Vector3:
	return Vector3(p2.x, ground(p2.x, p2.y) + r * 0.9, -p2.y)

func refresh(env: Dictionary, cells: Array, pos_of: Callable, tint_of: Callable, selected: int, meals: Array, colonies: Dictionary) -> void:
	var lvl: float = (float(env.get("tide_level", 0.5)) - ELEV.x) * RISE
	water.position.y = lvl
	var day: float = clamp(float(env.get("visible", 1.0)) * 1.4 + 0.08, 0.0, 1.0)
	water_mat.set_shader_parameter("swell", 0.15 + 0.6 * float(env.get("cloud", 0.0)))
	water_mat.set_shader_parameter("daylight", day)
	rock_mat.set_shader_parameter("water_level", lvl)
	rock_mat.set_shader_parameter("daylight", day)
	rig.update(env, cam.global_position.y, lvl)
	# two instances per cell: membrane, then genome core
	mm.instance_count = cells.size() * 2
	var centres := {}
	for i in cells.size():
		var c: Dictionary = cells[i]
		var r: float = max(c.r, 0.6)
		var p := cell_pos(pos_of.call(c), r)
		centres[c.id] = p
		var col: Color = tint_of.call(c)
		var sel := 1.0 if c.id == selected else 0.0
		mm.set_instance_transform(2 * i, Transform3D(Basis().scaled(Vector3.ONE * r), p))
		mm.set_instance_color(2 * i, col)
		mm.set_instance_custom_data(2 * i, Color(sel, 0, 0, 0))
		mm.set_instance_transform(2 * i + 1, Transform3D(Basis().scaled(Vector3.ONE * r * 0.32), p))
		mm.set_instance_color(2 * i + 1, col)
		mm.set_instance_custom_data(2 * i + 1, Color(sel, 1, 0, 0))
	bridge_mesh.clear_surfaces()
	var any := false
	for k in colonies:
		var ids: Array = colonies[k]
		for j in range(1, ids.size()):
			if not (centres.has(ids[j - 1]) and centres.has(ids[j])):
				continue
			var a: Vector3 = centres[ids[j - 1]]
			var b: Vector3 = centres[ids[j]]
			# the shore wraps (y is periodic): a colony across the seam would bridge the whole beach
			if abs(a.z - b.z) > length * 0.5:
				continue
			if not any:
				bridge_mesh.surface_begin(Mesh.PRIMITIVE_LINES)
				any = true
			bridge_mesh.surface_add_vertex(a)
			bridge_mesh.surface_add_vertex(b)
	if any:
		bridge_mesh.surface_end()
	flashes.instance_count = meals.size()
	for i in meals.size():
		var m: Dictionary = meals[i]
		var p2: Vector2 = pos_of.call({"id": m.id, "patch": m.patch, "y": m.prey_y})
		var age: float = m.age
		var s := 1.0 + 6.0 * age
		flashes.set_instance_transform(i, Transform3D(Basis().scaled(Vector3.ONE * s), cell_pos(p2, 1.0)))
		flashes.set_instance_color(i, Color(1.0, 0.25, 0.15, clamp(1.0 - age / 0.5, 0.0, 1.0) * 0.5))
	pitch = clamp(pitch, -1.45, 0.2)
	dist = clamp(dist, 8.0, 800.0)
	cam.position = target + Vector3(sin(yaw) * cos(pitch), -sin(pitch), cos(yaw) * cos(pitch)) * dist
	cam.look_at(target, Vector3.UP)

## Nearest cell id whose centre the ray passes within its radius; -1 for none.
func pick(screen: Vector2, cells: Array, pos_of: Callable) -> int:
	var from := cam.project_ray_origin(screen)
	var dir := cam.project_ray_normal(screen)
	var best := -1
	var bt := 1e9
	for c in cells:
		var r: float = max(c.r, 0.6)
		var p := cell_pos(pos_of.call(c), r) - from
		var t := p.dot(dir)
		if t > 0.0 and t < bt and (p - dir * t).length() < r:
			bt = t
			best = c.id
	return best

## Orbit and pan; returns true when the event was used.
func handle(e: InputEvent) -> bool:
	if e is InputEventMouseButton:
		if e.button_index == MOUSE_BUTTON_WHEEL_UP and e.pressed:
			dist *= 0.88
			return true
		if e.button_index == MOUSE_BUTTON_WHEEL_DOWN and e.pressed:
			dist /= 0.88
			return true
		if e.button_index == MOUSE_BUTTON_RIGHT:
			orbiting = e.pressed and not e.shift_pressed
			panning = e.pressed and e.shift_pressed
			return true
		if e.button_index == MOUSE_BUTTON_MIDDLE:
			panning = e.pressed
			return true
	elif e is InputEventMouseMotion:
		if orbiting:
			yaw -= e.relative.x * 0.005
			pitch -= e.relative.y * 0.005
			return true
		if panning:
			var right := cam.global_transform.basis.x
			var fwd := Vector3(-sin(yaw), 0.0, -cos(yaw))
			target += (-right * e.relative.x + fwd * e.relative.y) * dist * 0.0015
			return true
	return false
