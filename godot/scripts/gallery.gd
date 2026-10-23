## A kept creature in 3D for the codex: its body as recorded when kept (cells at their lattice
## places, tinted by what each was doing), turning slowly in its own little world.
extends SubViewportContainer

const ROLE_COLORS := [Color(0.25, 0.8, 0.85), Color(0.35, 0.8, 0.3), Color(0.95, 0.88, 0.72), Color(0.75, 0.22, 0.32), Color(1.0, 0.75, 0.2)]

var vp: SubViewport
var pivot: Node3D
var mm: MultiMesh
var cam: Camera3D
var radius := 4.0

func _ready() -> void:
	stretch = true
	vp = SubViewport.new()
	vp.own_world_3d = true
	vp.transparent_bg = true
	vp.msaa_3d = Viewport.MSAA_4X
	add_child(vp)
	var env := Environment.new()
	env.background_mode = Environment.BG_CLEAR_COLOR
	env.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	env.ambient_light_color = Color(0.5, 0.6, 0.75)
	env.ambient_light_energy = 0.3
	env.tonemap_mode = Environment.TONE_MAPPER_ACES
	env.glow_enabled = true
	env.glow_intensity = 0.6
	var we := WorldEnvironment.new()
	we.environment = env
	vp.add_child(we)
	var sun := DirectionalLight3D.new()
	sun.rotation = Vector3(-0.9, 0.6, 0.0)
	sun.light_energy = 1.0
	vp.add_child(sun)
	var rim := DirectionalLight3D.new()
	rim.rotation = Vector3(-0.2, 3.4, 0.0)
	rim.light_energy = 0.6
	rim.light_color = Color(0.6, 0.8, 1.0)
	vp.add_child(rim)
	pivot = Node3D.new()
	vp.add_child(pivot)
	var sphere := SphereMesh.new()
	sphere.radius = 0.62
	sphere.height = 1.24
	sphere.radial_segments = 16
	sphere.rings = 8
	var mat := StandardMaterial3D.new()
	mat.vertex_color_use_as_albedo = true
	mat.roughness = 0.35
	mat.rim_enabled = true
	mat.rim = 0.4
	sphere.material = mat
	mm = MultiMesh.new()
	mm.transform_format = MultiMesh.TRANSFORM_3D
	mm.use_colors = true
	mm.mesh = sphere
	var mi := MultiMeshInstance3D.new()
	mi.multimesh = mm
	pivot.add_child(mi)
	# the rock it stood on
	var rock := MeshInstance3D.new()
	var cyl := CylinderMesh.new()
	cyl.height = 0.4
	rock.mesh = cyl
	var rm := StandardMaterial3D.new()
	rm.albedo_color = Color(0.35, 0.3, 0.27)
	rm.roughness = 0.9
	rock.material_override = rm
	rock.name = "Rock"
	pivot.add_child(rock)
	cam = Camera3D.new()
	cam.fov = 35.0
	vp.add_child(cam)

## A cell's colour from the traits it expresses more than its body's average: the body's jobs
## become distinct colours (the reef view compares to the whole reef instead).
func tint(e: Array, mean: Array) -> Color:
	var c := Color(0, 0, 0)
	var tot := 0.001
	for k in 5:
		# floor: a trace trait over a near-zero mean is noise, not a job
		var w: float = pow(max(float(e[k]), 0.0) / max(float(mean[k]), 0.1), 4.0)
		c += ROLE_COLORS[k] * w
		tot += w
	if tot < 0.01:
		return Color(0.6, 0.62, 0.66)
	c = c / tot
	# deepen: pastel mixes read as one colour
	c = Color.from_hsv(c.h, min(1.0, c.s * 1.5 + 0.15), c.v * 0.9)
	return c

## `shape`: 8 floats per cell (x, y, height above the lowest cell, then 5 traits), as
## ReefSim.organism gives it. Empty: nothing to show.
func show_shape(shape: Array) -> void:
	var n := shape.size() / 8
	mm.instance_count = n
	pivot.get_node("Rock").visible = n > 0
	if n == 0:
		return
	var lo := Vector3(INF, INF, INF)
	var hi := -lo
	var pts := []
	for i in n:
		# lattice x along, y up the beach, z height: Godot y is up
		var p := Vector3(float(shape[i * 8]), float(shape[i * 8 + 2]), -float(shape[i * 8 + 1]))
		pts.append(p)
		lo = lo.min(p)
		hi = hi.max(p)
	var mid := (lo + hi) * 0.5
	mid.y = lo.y
	var mean := [0.0, 0.0, 0.0, 0.0, 0.0]
	for i in n:
		for k in 5:
			mean[k] += float(shape[i * 8 + 3 + k]) / n
	for i in n:
		var e := [shape[i * 8 + 3], shape[i * 8 + 4], shape[i * 8 + 5], shape[i * 8 + 6], shape[i * 8 + 7]]
		mm.set_instance_transform(i, Transform3D(Basis(), pts[i] - mid))
		mm.set_instance_color(i, tint(e, mean))
	var ext := hi - lo
	radius = max(2.5, max(ext.x, max(ext.y, ext.z)) * 0.5 + 1.0)
	var rock: MeshInstance3D = pivot.get_node("Rock")
	var cyl: CylinderMesh = rock.mesh
	cyl.top_radius = max(ext.x, ext.z) * 0.6 + 1.0
	cyl.bottom_radius = cyl.top_radius * 1.1
	rock.position = Vector3(0, -0.8, 0)

func _process(delta: float) -> void:
	if not is_visible_in_tree():
		return
	pivot.rotate_y(delta * 0.4)
	var d := radius * 4.6
	cam.position = Vector3(0, radius * 1.1, d)
	cam.look_at(Vector3(0, radius * 0.45, 0), Vector3.UP)
