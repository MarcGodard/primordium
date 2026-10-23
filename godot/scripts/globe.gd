## The coast's planet in 3D: one continent whose west coast runs north to south along longitude
## 0, the 12 sites as beads just offshore (tint = the trait a site leads in, size = population),
## lit by the sun at this instant of day and season, with both moons in their phases.
## Click a bead to select its site.
extends SubViewportContainer

signal site_clicked(k: int)

var vp: SubViewport
var cam: Camera3D
var sun: DirectionalLight3D
var planet: MeshInstance3D
var clouds_mat: ShaderMaterial
var atmo_mat: ShaderMaterial
var beads: Array = []
var bead_mats: Array = []
var moons: Array = []
var ring: MeshInstance3D
var lats: Array = []
var yaw := 0.0
var dragging := false

## Coastline longitude at a latitude (radians); matches planet.gdshader.
static func coast(lat: float) -> float:
	return 0.06 * sin(lat * 9.0) + 0.04 * sin(lat * 23.0 + 1.0) + 0.02

static func surface(lat: float, lon: float, r: float) -> Vector3:
	return Vector3(cos(lat) * sin(lon), sin(lat), cos(lat) * cos(lon)) * r

func _ready() -> void:
	stretch = true
	vp = SubViewport.new()
	vp.own_world_3d = true
	vp.transparent_bg = true
	vp.msaa_3d = Viewport.MSAA_4X
	add_child(vp)
	var we := WorldEnvironment.new()
	var e := Environment.new()
	e.background_mode = Environment.BG_CLEAR_COLOR
	e.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	e.ambient_light_color = Color(0.25, 0.3, 0.45)
	e.ambient_light_energy = 0.3
	e.tonemap_mode = Environment.TONE_MAPPER_ACES
	e.glow_enabled = true
	e.glow_intensity = 0.8
	we.environment = e
	vp.add_child(we)
	cam = Camera3D.new()
	cam.fov = 32.0
	vp.add_child(cam)
	sun = DirectionalLight3D.new()
	sun.light_energy = 1.6
	vp.add_child(sun)
	var sm := SphereMesh.new()
	sm.radius = 1.0
	sm.height = 2.0
	sm.radial_segments = 96
	sm.rings = 48
	planet = MeshInstance3D.new()
	planet.mesh = sm
	var pm := ShaderMaterial.new()
	pm.shader = load("res://shaders/planet.gdshader")
	planet.material_override = pm
	vp.add_child(planet)
	var cl := MeshInstance3D.new()
	var cm := SphereMesh.new()
	cm.radius = 1.012
	cm.height = 2.024
	cm.radial_segments = 64
	cm.rings = 32
	cl.mesh = cm
	clouds_mat = ShaderMaterial.new()
	clouds_mat.shader = load("res://shaders/clouds.gdshader")
	cl.material_override = clouds_mat
	vp.add_child(cl)
	var at := MeshInstance3D.new()
	var am := SphereMesh.new()
	am.radius = 1.07
	am.height = 2.14
	am.radial_segments = 64
	am.rings = 32
	at.mesh = am
	atmo_mat = ShaderMaterial.new()
	atmo_mat.shader = load("res://shaders/atmosphere.gdshader")
	at.material_override = atmo_mat
	vp.add_child(at)
	for k in 2:
		var m := MeshInstance3D.new()
		var mm := SphereMesh.new()
		mm.radius = 0.11 if k == 0 else 0.06
		mm.height = mm.radius * 2.0
		m.mesh = mm
		var mat := StandardMaterial3D.new()
		mat.albedo_color = Color(0.75, 0.74, 0.7)
		mat.roughness = 1.0
		m.material_override = mat
		vp.add_child(m)
		moons.append(m)
	ring = MeshInstance3D.new()
	var tm := TorusMesh.new()
	tm.inner_radius = 0.03
	tm.outer_radius = 0.038
	ring.mesh = tm
	var rmat := StandardMaterial3D.new()
	rmat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	rmat.albedo_color = Color(1, 1, 1)
	ring.material_override = rmat
	ring.visible = false
	vp.add_child(ring)

## Called each frame by the coast view.
func refresh(env: Dictionary, sites: Array, tints: Array, selected: int) -> void:
	if env.is_empty():
		return
	while beads.size() < sites.size():
		var b := MeshInstance3D.new()
		var s := SphereMesh.new()
		s.radius = 1.0
		s.height = 2.0
		b.mesh = s
		var m := StandardMaterial3D.new()
		m.emission_enabled = true
		b.material_override = m
		vp.add_child(b)
		beads.append(b)
		bead_mats.append(m)
	# subsolar point: declination by season, longitude by the hour at the coast
	var dec: float = env.get("declination", 0.0)
	var h: float = (env.day_phase - 0.5) * TAU
	var sun_dir := surface(dec, -h, 1.0)
	sun.look_at_from_position(Vector3.ZERO, -sun_dir, Vector3.UP if abs(sun_dir.y) < 0.99 else Vector3.FORWARD)
	sun.light_color = Color(1.0, 0.95, 0.88).lerp(Color(1.0, 0.75, 1.0), clamp(env.flare, 0.0, 1.0))
	atmo_mat.set_shader_parameter("sun_dir", sun_dir)
	clouds_mat.set_shader_parameter("cover", env.cloud)
	clouds_mat.set_shader_parameter("t", env.t_days)
	for k in 2:
		var ph: float = env["moon%d_phase" % k]
		var r := 2.0 + 0.8 * k
		var a: float = -h + ph * TAU + PI
		moons[k].position = Vector3(sin(a), 0.12 * sin(a * 0.7 + k), cos(a)) * r
	for k in sites.size():
		var s: Dictionary = sites[k]
		var lat: float = s.latitude
		var p := surface(lat, coast(lat) - 0.045, 1.004)
		var size: float = 0.008 + 0.012 * sqrt(clamp(s.cells / 200.0, 0.0, 1.5))
		beads[k].position = p
		beads[k].scale = Vector3.ONE * size
		var c: Color = tints[k]
		bead_mats[k].albedo_color = c
		bead_mats[k].emission = c
		bead_mats[k].emission_energy_multiplier = 1.4 if s.lineage else 0.8
		if k == selected:
			ring.visible = true
			ring.position = p * 1.01
			ring.look_at(p * 2.0, Vector3.UP if abs(p.y) < 0.99 else Vector3.FORWARD)
			ring.rotate_object_local(Vector3.RIGHT, PI / 2)
	if selected < 0:
		ring.visible = false
	# view from halfway between the coast and the sun: the coast stays in view and the
	# terminator crosses it as the day turns; right-drag adds an offset
	var view_lon: float = clamp(wrapf(-h, -PI, PI) * 0.5, -0.8, 0.8) + yaw
	cam.position = Vector3(sin(view_lon), 0.18, cos(view_lon)).normalized() * 4.6
	cam.look_at(Vector3.ZERO, Vector3.UP)

func _gui_input(e: InputEvent) -> void:
	if e is InputEventMouseButton:
		if e.button_index == MOUSE_BUTTON_RIGHT:
			dragging = e.pressed
			accept_event()
		elif e.button_index == MOUSE_BUTTON_LEFT and e.pressed:
			var best := -1
			var bd := 16.0
			for k in beads.size():
				var p: Vector3 = beads[k].position
				# beads on the far side are hidden by the planet
				if p.dot(cam.position) <= 0.0:
					continue
				var d: float = cam.unproject_position(p).distance_to(e.position)
				if d < bd:
					bd = d
					best = k
			if best >= 0:
				site_clicked.emit(best)
			accept_event()
	elif e is InputEventMouseMotion and dragging:
		yaw -= e.relative.x * 0.01
		accept_event()
