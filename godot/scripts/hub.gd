## The hub: the planet in space with the seven scales orbiting it as small worlds, smallest to
## largest, numbered as the views number them (-1 elements .. 5 reef), plus the player's own
## world (the garden). Locked worlds sit dark until the campaign opens them. Hover a world for
## what it is, click to dive in. `chosen(k)` asks main to travel.
extends Node3D

signal chosen(k: int)

const SCALES := [
	["Elements", "Where atoms come from: a periodic table grown from one shell rule.", Color(0.95, 0.75, 0.35)],
	["Chemistry", "Atoms on a sunlit beach bond, break and glow with the tide.", Color(0.55, 0.75, 1.0)],
	["Polymers", "Chains that fold into catalysts, and copy themselves in the dry.", Color(0.45, 0.95, 0.85)],
	["Protocells", "Oily bubbles that trap chains, grow and split.", Color(0.95, 0.85, 0.6)],
	["Cells", "Genomes, a genetic code, mutation and the first predators.", Color(0.55, 0.95, 0.5)],
	["Coast", "Twelve beaches along an Earth-sized planet adapt to their climate.", Color(0.4, 0.7, 1.0)],
	["Reef", "Many-celled bodies in 3D: plants, crawlers, lineages, evolution.", Color(1.0, 0.5, 0.65)],
	["Your world", "Your own reef: the creatures you kept live, compete and keep evolving.", Color(0.75, 1.0, 0.55)],
]
const GARDEN := 7

## Number shown before a world's name; the garden is not a scale.
static func label(k: int) -> String:
	return SCALES[k][0] if k == GARDEN else "%d  %s" % [k - 1, SCALES[k][0]]
const RING := 8.5

var cam: Camera3D
var world_env: WorldEnvironment
var environment: Environment
var ring: Node3D
var orbs: Array = []
var hovered := -1
var layer: CanvasLayer
var card: Label
var card_sub: Label
var t := 0.0
var sun_dir := Vector3(0.6, 0.3, 0.75).normalized()
## Off in captures: the mouse over the window would otherwise pop a hover card.
var hover_enabled := true
var campaign: Node
var foot: Label

func _ready() -> void:
	environment = Environment.new()
	environment.background_mode = Environment.BG_SKY
	var sky := Sky.new()
	var sm := ShaderMaterial.new()
	sm.shader = load("res://shaders/space.gdshader")
	sky.sky_material = sm
	environment.sky = sky
	environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	environment.ambient_light_color = Color(0.35, 0.4, 0.55)
	environment.ambient_light_energy = 0.25
	environment.tonemap_mode = Environment.TONE_MAPPER_ACES
	environment.glow_enabled = true
	environment.glow_intensity = 1.0
	environment.glow_bloom = 0.2
	world_env = WorldEnvironment.new()
	add_child(world_env)
	var sun := DirectionalLight3D.new()
	sun.look_at_from_position(Vector3.ZERO, -sun_dir, Vector3.UP)
	sun.light_energy = 1.6
	add_child(sun)
	cam = Camera3D.new()
	cam.fov = 45.0
	add_child(cam)
	_build_planet()
	ring = Node3D.new()
	ring.rotation = Vector3(0.28, 0.0, 0.0)
	add_child(ring)
	for k in SCALES.size():
		var orb := _emblem(k)
		ring.add_child(orb)
		orbs.append(orb)
	_build_overlay()

func _build_planet() -> void:
	var sm := SphereMesh.new()
	sm.radius = 3.0
	sm.height = 6.0
	sm.radial_segments = 96
	sm.rings = 48
	var planet := MeshInstance3D.new()
	planet.mesh = sm
	var pm := ShaderMaterial.new()
	pm.shader = load("res://shaders/planet.gdshader")
	planet.material_override = pm
	planet.name = "Planet"
	add_child(planet)
	var cm := SphereMesh.new()
	cm.radius = 3.012
	cm.height = 6.024
	cm.radial_segments = 64
	cm.rings = 32
	var cl := MeshInstance3D.new()
	cl.mesh = cm
	var cmat := ShaderMaterial.new()
	cmat.shader = load("res://shaders/clouds.gdshader")
	cmat.set_shader_parameter("cover", 0.45)
	cl.material_override = cmat
	cl.name = "Clouds"
	add_child(cl)
	var am := SphereMesh.new()
	am.radius = 3.09
	am.height = 6.18
	am.radial_segments = 64
	am.rings = 32
	var at := MeshInstance3D.new()
	at.mesh = am
	var amat := ShaderMaterial.new()
	amat.shader = load("res://shaders/atmosphere.gdshader")
	amat.set_shader_parameter("sun_dir", sun_dir)
	at.material_override = amat
	add_child(at)

func _mat(c: Color, glow: float = 0.6, alpha: float = 1.0) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.albedo_color = Color(c, alpha)
	m.emission_enabled = true
	m.emission = c
	m.emission_energy_multiplier = glow
	m.roughness = 0.35
	if alpha < 1.0:
		m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	return m

func _ball(parent: Node3D, pos: Vector3, r: float, m: Material) -> MeshInstance3D:
	var s := SphereMesh.new()
	s.radius = r
	s.height = r * 2.0
	s.radial_segments = 16
	s.rings = 8
	var mi := MeshInstance3D.new()
	mi.mesh = s
	mi.material_override = m
	mi.position = pos
	parent.add_child(mi)
	return mi

func _torus(parent: Node3D, r: float, w: float, m: Material, rot: Vector3) -> void:
	var tm := TorusMesh.new()
	tm.inner_radius = r - w
	tm.outer_radius = r + w
	tm.rings = 48
	var mi := MeshInstance3D.new()
	mi.mesh = tm
	mi.material_override = m
	mi.rotation = rot
	parent.add_child(mi)

func _stick(parent: Node3D, a: Vector3, b: Vector3, m: Material) -> void:
	var cy := CylinderMesh.new()
	cy.top_radius = 0.05
	cy.bottom_radius = 0.05
	cy.height = a.distance_to(b)
	var mi := MeshInstance3D.new()
	mi.mesh = cy
	mi.material_override = m
	parent.add_child(mi)
	mi.position = (a + b) * 0.5
	var up := (b - a).normalized()
	var side := up.cross(Vector3.FORWARD if abs(up.dot(Vector3.FORWARD)) < 0.9 else Vector3.RIGHT).normalized()
	mi.basis = Basis(side, up, side.cross(up))

## A small 3D emblem of each scale, in its colour, inside a faint halo.
func _emblem(k: int) -> Node3D:
	var c: Color = SCALES[k][2]
	var root := Node3D.new()
	var body := Node3D.new()
	body.name = "Body"
	root.add_child(body)
	match k:
		0:
			for i in 6:
				_ball(body, Vector3(randf() - 0.5, randf() - 0.5, randf() - 0.5) * 0.35, 0.16, _mat(Color(0.95, 0.35, 0.3) if i % 2 == 0 else Color(0.6, 0.65, 0.8), 0.3))
			_torus(body, 0.75, 0.015, _mat(c, 1.5), Vector3(0.6, 0, 0))
			_torus(body, 1.05, 0.015, _mat(c, 1.5), Vector3(-0.5, 0, 0.9))
			_ball(body, Vector3(0.75, 0, 0), 0.07, _mat(Color(0.5, 1, 1), 4.0))
			_ball(body, Vector3(-0.6, 0.6, 0.3), 0.07, _mat(Color(0.5, 1, 1), 4.0))
		1:
			var pts := [Vector3(0, 0, 0), Vector3(0.6, 0.3, 0), Vector3(-0.55, 0.35, 0.2), Vector3(0.1, -0.6, 0.3), Vector3(0.9, -0.3, -0.3)]
			var cols := [c, Color(1, 0.6, 0.4), Color(0.7, 1, 0.6), Color(1, 1, 1), Color(0.9, 0.5, 1)]
			for i in pts.size():
				_ball(body, pts[i], 0.24, _mat(cols[i], 0.5))
			for e in [[0, 1], [0, 2], [0, 3], [1, 4]]:
				_stick(body, pts[e[0]], pts[e[1]], _mat(Color(0.9, 0.9, 1.0), 0.8))
		2:
			for i in 9:
				var a := i * 0.7
				var p := Vector3(cos(a) * 0.55, (i - 4) * 0.2, sin(a) * 0.55)
				_ball(body, p, 0.14, _mat(c, 0.7))
				_ball(body, p * 1.45, 0.08, _mat(Color.from_hsv(fmod(i * 0.23, 1.0), 0.6, 1.0), 1.0))
		3:
			_ball(body, Vector3.ZERO, 0.95, _mat(c, 0.25, 0.28))
			for i in 5:
				_ball(body, Vector3(randf() - 0.5, randf() - 0.5, randf() - 0.5) * 0.9, 0.12, _mat(Color(0.5, 0.95, 0.9), 1.2))
		4:
			_ball(body, Vector3.ZERO, 0.9, _mat(c, 0.25, 0.4))
			_torus(body, 0.45, 0.04, _mat(Color(0.3, 0.9, 0.95), 1.8), Vector3(1.2, 0.3, 0))
			_ball(body, Vector3(0.1, 0.05, 0.1), 0.2, _mat(Color(0.95, 0.6, 0.25), 0.8))
		5:
			var s := SphereMesh.new()
			s.radius = 0.85
			s.height = 1.7
			var mi := MeshInstance3D.new()
			mi.mesh = s
			var pm := ShaderMaterial.new()
			pm.shader = load("res://shaders/planet.gdshader")
			mi.material_override = pm
			body.add_child(mi)
		6:
			var cols := [Color(0.35, 0.8, 0.3), Color(0.75, 0.22, 0.32), Color(0.95, 0.88, 0.72), Color(1.0, 0.75, 0.2), Color(0.25, 0.8, 0.85)]
			for i in 26:
				var h := float(i % 5)
				var a := i * 2.4
				var rr := 0.55 - h * 0.09
				_ball(body, Vector3(cos(a) * rr, -0.5 + h * 0.27, sin(a) * rr), 0.15, _mat(cols[i % 5], 0.5))
		7:
			# a few bodies of different lineages on one small rock
			_ball(body, Vector3(0, -0.55, 0), 0.6, _mat(Color(0.35, 0.3, 0.28), 0.1))
			for j in 4:
				var base := Vector3(cos(j * 1.57) * 0.4, -0.1, sin(j * 1.57) * 0.4)
				var col := Color.from_hsv(0.12 + j * 0.22, 0.6, 1.0)
				for i in 3 + j:
					_ball(body, base + Vector3(0, i * 0.17, 0), 0.11, _mat(col, 0.8))
			_torus(body, 0.95, 0.02, _mat(c, 2.0), Vector3(1.4, 0, 0))
	var halo := MeshInstance3D.new()
	var hs := SphereMesh.new()
	hs.radius = 1.35
	hs.height = 2.7
	halo.mesh = hs
	var hm := StandardMaterial3D.new()
	hm.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	hm.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	hm.blend_mode = BaseMaterial3D.BLEND_MODE_ADD
	hm.albedo_color = Color(c, 0.07)
	hm.cull_mode = BaseMaterial3D.CULL_FRONT
	halo.mesh = hs
	halo.material_override = hm
	halo.name = "Halo"
	root.add_child(halo)
	var lbl := Label3D.new()
	lbl.text = label(k)
	lbl.name = "Label"
	lbl.font_size = 64
	lbl.pixel_size = 0.009
	lbl.billboard = BaseMaterial3D.BILLBOARD_ENABLED
	lbl.position = Vector3(0, -1.75, 0)
	lbl.modulate = Color(c, 0.95)
	lbl.outline_size = 8
	lbl.outline_modulate = Color(0, 0, 0, 0.8)
	root.add_child(lbl)
	return root

func _build_overlay() -> void:
	layer = CanvasLayer.new()
	layer.layer = 3
	add_child(layer)
	var title := Label.new()
	title.text = "PRIMORDIUM"
	title.add_theme_font_size_override("font_size", 54)
	title.add_theme_color_override("font_color", Color(0.85, 0.92, 1.0))
	title.anchor_left = 0.5
	title.anchor_right = 0.5
	title.offset_left = -400
	title.offset_right = 400
	title.offset_top = 26
	title.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	layer.add_child(title)
	var sub := Label.new()
	sub.text = "from atoms to animals: a world that evolves on its own"
	sub.add_theme_font_size_override("font_size", 18)
	sub.add_theme_color_override("font_color", Color(0.6, 0.72, 0.9))
	sub.anchor_left = 0.5
	sub.anchor_right = 0.5
	sub.offset_left = -400
	sub.offset_right = 400
	sub.offset_top = 96
	sub.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	layer.add_child(sub)
	card = Label.new()
	card.add_theme_font_size_override("font_size", 30)
	card.anchor_left = 0.5
	card.anchor_right = 0.5
	card.anchor_top = 1.0
	card.anchor_bottom = 1.0
	card.offset_left = -500
	card.offset_right = 500
	card.offset_top = -150
	card.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	layer.add_child(card)
	card_sub = Label.new()
	card_sub.add_theme_font_size_override("font_size", 18)
	card_sub.add_theme_color_override("font_color", Color(0.75, 0.82, 0.95))
	card_sub.anchor_left = 0.5
	card_sub.anchor_right = 0.5
	card_sub.anchor_top = 1.0
	card_sub.anchor_bottom = 1.0
	card_sub.offset_left = -500
	card_sub.offset_right = 500
	card_sub.offset_top = -108
	card_sub.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	layer.add_child(card_sub)
	foot = Label.new()
	foot.add_theme_font_size_override("font_size", 14)
	foot.add_theme_color_override("font_color", Color(0.5, 0.6, 0.72))
	foot.anchor_left = 0.5
	foot.anchor_right = 0.5
	foot.anchor_top = 1.0
	foot.anchor_bottom = 1.0
	foot.offset_left = -500
	foot.offset_right = 500
	foot.offset_left = -600
	foot.offset_right = 600
	foot.offset_top = -48
	foot.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	layer.add_child(foot)

func activate(on: bool) -> void:
	visible = on
	set_process(on)
	set_process_unhandled_input(on)
	layer.visible = on
	world_env.environment = environment if on else null
	if on:
		cam.make_current()
	else:
		cam.clear_current(false)

## Position of world k on the ring (world space).
func orb_pos(k: int) -> Vector3:
	return orbs[k].global_position

func _process(delta: float) -> void:
	t += delta
	ring.rotation.y = t * 0.07
	var n := orbs.size()
	for k in n:
		var a := TAU * k / n
		orbs[k].position = Vector3(cos(a) * RING, sin(t * 0.6 + k) * 0.25, sin(a) * RING)
		var body: Node3D = orbs[k].get_node("Body")
		body.rotate_y(delta * (0.5 + 0.1 * k))
		var target_s := 1.35 if k == hovered else 1.0
		orbs[k].scale = orbs[k].scale.lerp(Vector3.ONE * target_s, min(1.0, delta * 8.0))
		var hm: StandardMaterial3D = orbs[k].get_node("Halo").material_override
		var open: bool = campaign == null or campaign.is_open(k)
		hm.albedo_color.a = (0.2 if k == hovered else 0.07) if open else 0.02
		body.visible = open or fmod(t + k, 3.0) < 0.12
		var lbl: Label3D = orbs[k].get_node("Label")
		lbl.modulate = Color(SCALES[k][2], 0.95) if open else Color(0.5, 0.52, 0.58, 0.7)
		lbl.text = label(k) if open else label(k) + "  (locked)"
	get_node("Planet").rotate_y(delta * 0.05)
	get_node("Clouds").rotate_y(delta * 0.065)
	var cp := Vector3(sin(t * 0.05) * 3.0, 9.0, 23.0)
	cam.position = cp
	cam.look_at(Vector3(0, -1.6, 0), Vector3.UP)
	_hover(get_viewport().get_mouse_position() if hover_enabled else Vector2(-1e6, -1e6))

func _hover(mp: Vector2) -> void:
	var best := -1
	var bd := 90.0
	for k in orbs.size():
		var p: Vector3 = orbs[k].global_position
		if cam.is_position_behind(p):
			continue
		var d := cam.unproject_position(p).distance_to(mp)
		if d < bd:
			bd = d
			best = k
	hovered = best
	var explore: bool = campaign != null and campaign.state.explore
	var done: int = campaign.state.done.size() if campaign else 0
	foot.text = "click a world (keys 1-8)   ·   J codex   ·   E %s   ·   in any scale: M back here, Tab / Shift+Tab next scale, H guide" % ("explore mode: all worlds open" if explore else "campaign: %d of %d goals" % [done, campaign.GOALS.size()])
	if best >= 0:
		var open: bool = campaign == null or campaign.is_open(best)
		card.text = ("Scale %d  ·  %s" % [best - 1, SCALES[best][0]]) if best != GARDEN else SCALES[best][0]
		card.add_theme_color_override("font_color", SCALES[best][2] if open else Color(0.6, 0.62, 0.7))
		if open:
			var g := ""
			if campaign and best < campaign.GOALS.size():
				g = "   Goal: " + campaign.goal_title(best) + (" (done)" if campaign.is_done(best) else "")
			card_sub.text = SCALES[best][1] + g
		elif best == GARDEN:
			card_sub.text = "Locked. Keep a creature at the reef (K) and it opens here."
		else:
			card_sub.text = "Locked. Reach the %s goal first: %s." % [SCALES[best - 1][0], campaign.goal_title(best - 1)]
	else:
		card.text = ""
		card_sub.text = "Seven scales of one world, from the atom to the reef, and a world of your own."

func _unhandled_input(e: InputEvent) -> void:
	if e is InputEventMouseButton and e.pressed and e.button_index == MOUSE_BUTTON_LEFT and hovered >= 0:
		chosen.emit(hovered)
		get_viewport().set_input_as_handled()
	elif e is InputEventKey and e.pressed and e.keycode >= KEY_1 and e.keycode <= KEY_8:
		chosen.emit(e.keycode - KEY_1)
		get_viewport().set_input_as_handled()
	elif e is InputEventKey and e.pressed and not e.echo and e.keycode == KEY_E and campaign:
		campaign.set_explore(not campaign.state.explore)
		get_viewport().set_input_as_handled()
