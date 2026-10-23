## Scale -1 view: the periodic table of this world, built from its shell rule, and a 3D atom of
## the selected element: nucleus, electron shells, and the empty slots that make it bond.
## Buttons preview other universes (more or bigger shells) and what life there would be made of.
extends Node3D

const EXPLAIN := """[b]Scale -1: the periodic table[/b]

[b]Where elements come from.[/b] An atom is a nucleus with electrons around it, arranged in [b]shells[/b]. Each shell holds a fixed number of electrons; when one is full, the next starts. In our universe the shells near the nucleus hold 2, then 8. In this world they hold [b]2, then 6[/b], so there are only 8 elements. Nothing in this table was typed in by hand: every number comes from that one rule.

[b]Why atoms bond.[/b] An atom is most stable with its outer shell completely full or completely empty. It shares electrons with neighbours to get there. How many it needs to share is its [b]valence[/b]: the number of bonds it makes. Atoms with a full shell (nobles, violet) bond with nothing. In the 3D atom, bright electrons are the outer shell and faint hollow dots are the empty slots it wants to fill.

[b]Why life here is built on Ka.[/b] To make a chain that carries information you need an atom that bonds [b]three[/b] ways: two bonds to its neighbours in the chain and one to a side group. Only Ka does that here (gold). The other bonding atoms hang off the chain as side groups: they are the [b]letters[/b] of every molecule of life in this world. In our universe carbon bonds four ways, which is why our biology branches and folds in other ways.

[b]Which letters carry genes.[/b] When a chain is copied, each letter attracts its strongest partner. Copy twice and most letters get rewritten; only Li and Fl come back as themselves (tagged [color=#8f8]gene[/color]). So genes here are written in binary, and the cell scale has to invent a genetic code to use all five letters.

[b]Electronegativity[/b] is how hard an atom pulls on shared electrons. Two atoms that pull very differently form strong, partly ionic bonds; atoms that pull alike stay oily and avoid water.

[b]Try.[/b] Click any element. Then press a universe button to see what a third shell, or our own octets, would change: more elements, a different backbone, a different genetic alphabet."""
const SCALE_NAME := "scale -1: elements"

const ROLE_COL := {
	"backbone": Color(1.0, 0.76, 0.25),
	"letter": Color(0.35, 0.75, 0.95),
	"rival": Color(1.0, 0.5, 0.25),
	"noble": Color(0.6, 0.5, 0.85),
}
const UNIVERSES := [[[2, 6], "This world (2, 6)"], [[2, 6, 6], "A third shell (2, 6, 6)"], [[2, 8, 8], "Our octets (2, 8, 8)"]]
const TILE := 2.7

var sim: ElementsSim
var cam: Camera3D
var env := {}
var elements: Array = []
var summary := {}
var tiles: Array = []
var table_root: Node3D
var atom_root: Node3D
var nucleus: Node3D
var electrons: Array = []
var selected := 0
var layer: CanvasLayer
var bars: Control
var title: Label
var world_env: WorldEnvironment
var environment: Environment
var target := Vector3(1.5, 0.0, 0.0)
var dist := 30.0
var yaw := 0.0
var t := 0.0

func _ready() -> void:
	sim = ElementsSim.new()
	add_child(sim)
	environment = Environment.new()
	environment.background_mode = Environment.BG_COLOR
	environment.background_color = Color(0.015, 0.02, 0.04)
	environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	environment.ambient_light_color = Color(0.4, 0.45, 0.6)
	environment.ambient_light_energy = 0.5
	environment.tonemap_mode = Environment.TONE_MAPPER_ACES
	environment.glow_enabled = true
	environment.glow_intensity = 0.9
	environment.glow_bloom = 0.15
	world_env = WorldEnvironment.new()
	add_child(world_env)
	var key := DirectionalLight3D.new()
	key.rotation = Vector3(-0.7, 0.5, 0.0)
	key.light_energy = 1.2
	add_child(key)
	cam = Camera3D.new()
	cam.fov = 42.0
	add_child(cam)
	table_root = Node3D.new()
	add_child(table_root)
	atom_root = Node3D.new()
	add_child(atom_root)
	_build_overlay()
	load_universe([])

func _build_overlay() -> void:
	layer = CanvasLayer.new()
	layer.layer = 2
	add_child(layer)
	var row := HBoxContainer.new()
	row.anchor_left = 0.5
	row.anchor_right = 0.5
	row.offset_left = -330
	row.offset_right = 330
	row.offset_top = 40
	row.add_theme_constant_override("separation", 10)
	layer.add_child(row)
	for u in UNIVERSES:
		var b := Button.new()
		b.text = u[1]
		b.focus_mode = Control.FOCUS_NONE
		var shells: Array = u[0]
		b.pressed.connect(func() -> void: load_universe(shells))
		row.add_child(b)
	title = Label.new()
	title.anchor_left = 0.5
	title.anchor_right = 0.5
	title.offset_left = -300
	title.offset_right = 300
	title.offset_top = 78
	title.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	title.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	title.add_theme_font_size_override("font_size", 14)
	title.add_theme_color_override("font_color", Color(0.75, 0.85, 1.0))
	layer.add_child(title)
	bars = Control.new()
	bars.anchor_left = 0.5
	bars.anchor_right = 0.5
	bars.anchor_top = 1.0
	bars.anchor_bottom = 1.0
	bars.offset_left = -360
	bars.offset_right = 360
	bars.offset_top = -190
	bars.offset_bottom = -12
	bars.mouse_filter = Control.MOUSE_FILTER_IGNORE
	bars.draw.connect(_draw_bars)
	layer.add_child(bars)

func load_universe(shells: Array) -> void:
	sim.load(PackedByteArray(shells))
	elements = sim.elements()
	summary = sim.summary()
	# start on the backbone: the atom life here is built on
	selected = summary.backbone - 1 if summary.backbone > 0 else 0
	_build_table()
	_build_atom()
	var names := func(zs: PackedByteArray) -> String:
		var out := []
		for z in zs:
			out.append(elements[z - 1].name)
		return " ".join(PackedStringArray(out))
	var s: PackedByteArray = sim.shells()
	var txt := "shells %s: %d elements.  " % [str(Array(s)), elements.size()]
	if summary.backbone == 0:
		txt += "Nothing bonds: no chemistry."
	else:
		var b: Dictionary = elements[summary.backbone - 1]
		txt += "Backbone %s (bonds %d ways)" % [b.name, b.valence]
		if b.valence < 3:
			txt += ": too few bonds for a chain with side groups, so no readable molecules."
		else:
			txt += ".  Letters: %s.  Genes written in: %s." % [names.call(summary.letters), names.call(summary.heritable)]
			if b.valence > 3:
				txt += "  Chains branch (%d side groups per link)." % (b.valence - 2)
	if not summary.this_world:
		txt += "  (Preview: the simulation runs on 2, 6.)"
	title.text = txt
	bars.queue_redraw()

## Rows are shells; a shell's last element (full shell, a noble) sits in the far-right column.
func _layout(e: Dictionary) -> Vector2:
	var s: PackedByteArray = sim.shells()
	var maxcap := 0
	for c in s:
		maxcap = max(maxcap, c)
	var col: int = e.outer - 1
	if e.outer == e.capacity:
		col = maxcap - 1
	return Vector2(col, e.shell - 1)

func _build_table() -> void:
	for c in table_root.get_children():
		c.queue_free()
	tiles.clear()
	var s: PackedByteArray = sim.shells()
	var maxcap := 0
	for c in s:
		maxcap = max(maxcap, c)
	var origin := Vector3(-(maxcap - 1) * TILE * 0.5 - 6.0, (s.size() - 1) * TILE * 0.5, 0.0)
	for i in elements.size():
		var e: Dictionary = elements[i]
		var g := _layout(e)
		var tile := Node3D.new()
		tile.position = origin + Vector3(g.x * TILE, -g.y * TILE, 0.0)
		table_root.add_child(tile)
		var box := MeshInstance3D.new()
		var bm := BoxMesh.new()
		bm.size = Vector3(TILE * 0.9, TILE * 0.9, 0.4)
		box.mesh = bm
		var m := StandardMaterial3D.new()
		var col: Color = ROLE_COL[e.role]
		m.albedo_color = col.darkened(0.45)
		m.emission_enabled = true
		m.emission = col
		m.emission_energy_multiplier = 0.35
		m.metallic = 0.3
		m.roughness = 0.35
		box.material_override = m
		tile.add_child(box)
		if e.heritable:
			# a genome letter: survives copying twice
			var gl := Label3D.new()
			gl.text = "gene"
			gl.font_size = 40
			gl.pixel_size = 0.0075
			gl.position = Vector3(TILE * 0.22, TILE * 0.3, 0.23)
			gl.modulate = Color(0.55, 1.0, 0.7)
			tile.add_child(gl)
		var sym := Label3D.new()
		sym.text = e.name
		sym.font_size = 120
		sym.pixel_size = 0.0075
		sym.position = Vector3(0, 0.15, 0.23)
		sym.outline_size = 0
		tile.add_child(sym)
		var z := Label3D.new()
		z.text = "%d" % e.z
		z.font_size = 48
		z.pixel_size = 0.0075
		z.position = Vector3(-TILE * 0.3, TILE * 0.3, 0.23)
		z.modulate = Color(1, 1, 1, 0.7)
		tile.add_child(z)
		var v := Label3D.new()
		v.text = "noble" if e.valence == 0 else "bonds %d" % e.valence
		v.font_size = 40
		v.pixel_size = 0.0075
		v.position = Vector3(0, -TILE * 0.3, 0.23)
		v.modulate = Color(1, 1, 1, 0.75)
		tile.add_child(v)
		tiles.append(tile)

## The selected element's atom: nucleus of protons (red) and as many neutrons (blue-grey,
## illustrative), shells as rings, electrons orbiting, empty outer slots as faint dots.
func _build_atom() -> void:
	for c in atom_root.get_children():
		c.queue_free()
	electrons.clear()
	if elements.is_empty():
		return
	var e: Dictionary = elements[selected]
	atom_root.position = Vector3(9.5, 0.5, 0.0)
	atom_root.scale = Vector3.ONE * 0.85
	nucleus = Node3D.new()
	atom_root.add_child(nucleus)
	var n: int = e.z * 2
	var r0: float = 0.32 * pow(n, 1.0 / 3.0)
	var ball := SphereMesh.new()
	ball.radius = 0.3
	ball.height = 0.6
	var pm := StandardMaterial3D.new()
	pm.albedo_color = Color(0.95, 0.3, 0.25)
	pm.roughness = 0.4
	var nm := StandardMaterial3D.new()
	nm.albedo_color = Color(0.55, 0.62, 0.75)
	nm.roughness = 0.4
	for k in n:
		var b := MeshInstance3D.new()
		b.mesh = ball
		b.material_override = pm if k % 2 == 0 else nm
		# golden-angle spiral packs the nucleons evenly in a ball
		var y: float = 1.0 - 2.0 * (k + 0.5) / n
		var rad := sqrt(max(0.0, 1.0 - y * y))
		var a: float = k * 2.39996
		b.position = Vector3(cos(a) * rad, y, sin(a) * rad) * r0 * (0.6 + 0.4 * fmod(k * 0.618, 1.0))
		nucleus.add_child(b)
	var em := SphereMesh.new()
	em.radius = 0.22
	em.height = 0.44
	var inner_m := StandardMaterial3D.new()
	inner_m.albedo_color = Color(0.5, 0.6, 1.0)
	inner_m.emission_enabled = true
	inner_m.emission = Color(0.45, 0.55, 1.0)
	inner_m.emission_energy_multiplier = 1.5
	var outer_m := inner_m.duplicate()
	outer_m.albedo_color = Color(0.5, 1.0, 1.0)
	outer_m.emission = Color(0.4, 1.0, 0.95)
	outer_m.emission_energy_multiplier = 3.0
	var hole_m := StandardMaterial3D.new()
	hole_m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	hole_m.albedo_color = Color(1, 1, 1, 0.12)
	hole_m.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	var s: PackedByteArray = sim.shells()
	for k in e.shell:
		var radius: float = 2.6 + 1.9 * k
		var count: int = s[k] if k < e.shell - 1 else e.outer
		var cap: int = s[k]
		var tilt := Basis(Vector3(1, 0, 0), 0.5 + k * 0.7) * Basis(Vector3(0, 0, 1), k * 0.9)
		var ring := MeshInstance3D.new()
		var tm := TorusMesh.new()
		tm.inner_radius = radius - 0.03
		tm.outer_radius = radius + 0.03
		tm.rings = 96
		ring.mesh = tm
		ring.basis = tilt
		var rm := StandardMaterial3D.new()
		rm.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
		rm.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
		rm.albedo_color = Color(0.6, 0.75, 1.0, 0.25)
		ring.material_override = rm
		atom_root.add_child(ring)
		for j in cap:
			var el := MeshInstance3D.new()
			el.mesh = em
			var outer: bool = k == e.shell - 1
			if j < count:
				el.material_override = outer_m if outer else inner_m
			elif outer:
				el.material_override = hole_m
			else:
				continue
			atom_root.add_child(el)
			electrons.append([el, radius, tilt, TAU * j / cap, 0.9 / (k + 1)])
	var lbl := Label3D.new()
	lbl.text = "%s  (Z = %d)" % [e.name, e.z]
	lbl.font_size = 96
	lbl.pixel_size = 0.01
	lbl.position = Vector3(0, -(2.6 + 1.9 * e.shell + 1.2), 0)
	lbl.billboard = BaseMaterial3D.BILLBOARD_ENABLED
	atom_root.add_child(lbl)
	for i in tiles.size():
		tiles[i].position.z = 0.6 if i == selected else 0.0

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

func speed() -> int:
	return 0

func warm(_n: int) -> void:
	pass

## Capture flag --pick: show the third-shell universe.
func pick_longest() -> void:
	load_universe(UNIVERSES[1][0])

func selected_info() -> Dictionary:
	return {}

func _process(delta: float) -> void:
	t += delta
	if nucleus:
		nucleus.rotate_y(delta * 0.4)
	for e in electrons:
		var a: float = e[3] + t * e[4]
		e[0].position = e[2] * Vector3(cos(a) * e[1], 0.0, sin(a) * e[1])
	var off := Vector3(sin(yaw) * 0.25, 0.12, cos(yaw)).normalized() * dist
	cam.position = target + off
	cam.look_at(target, Vector3.UP)
	yaw = sin(t * 0.15) * 0.25

func _unhandled_input(ev: InputEvent) -> void:
	if ev is InputEventMouseButton and ev.pressed:
		if ev.button_index == MOUSE_BUTTON_WHEEL_UP:
			dist = max(14.0, dist * 0.9)
		elif ev.button_index == MOUSE_BUTTON_WHEEL_DOWN:
			dist = min(80.0, dist / 0.9)
		elif ev.button_index == MOUSE_BUTTON_LEFT:
			var best := -1
			var bd := 60.0
			for i in tiles.size():
				var p: Vector2 = cam.unproject_position(tiles[i].global_position)
				var d := p.distance_to(ev.position)
				if d < bd:
					bd = d
					best = i
			if best >= 0:
				select(best)
	elif ev is InputEventKey and ev.pressed:
		if ev.keycode == KEY_RIGHT:
			select((selected + 1) % elements.size())
		elif ev.keycode == KEY_LEFT:
			select((selected - 1 + elements.size()) % elements.size())

func select(i: int) -> void:
	selected = i
	_build_atom()
	bars.queue_redraw()

func hud_text() -> String:
	if elements.is_empty():
		return ""
	var e: Dictionary = elements[selected]
	var config := []
	var s: PackedByteArray = sim.shells()
	for k in e.shell:
		config.append(str(s[k] if k < e.shell - 1 else e.outer))
	var t := "[b][font_size=22]%s[/font_size][/b]  element %d\n" % [e.name, e.z]
	t += "electrons by shell: %s  (outer shell %d of %d)\n" % [" · ".join(PackedStringArray(config)), e.outer, e.capacity]
	t += "bonds it makes (valence): [b]%d[/b]\n" % e.valence
	t += "electronegativity: %.2f   %s\n" % [e.chi, "likes water" if e.hydropathy > 0.15 else ("oily" if e.hydropathy < -0.05 else "neutral")]
	if e.has("abundance"):
		t += "share of atoms on the beach: %.0f%%\n" % (e.abundance * 100.0)
	t += "\n"
	match e.role:
		"backbone":
			t += "[color=#ffc240][b]Backbone.[/b][/color] Bonds %d ways: two to its neighbours in a chain, the rest to side groups. Every chain of life here is built on it.\n" % e.valence
		"letter", "rival":
			if e.role == "rival":
				t += "[color=#ff8040][b]Rival backbone.[/b][/color] As bonded as the backbone; as a side group it would branch a chain.\n"
			else:
				t += "[color=#5ac0f2][b]Letter.[/b][/color] Bonds %s: it hangs off a backbone as a side group, one letter in a chain's sequence.\n" % ("once" if e.valence == 1 else "%d ways" % e.valence)
			if e.partner > 0:
				t += "When chains are copied it pairs with [b]%s[/b].\n" % elements[e.partner - 1].name
			if e.heritable:
				t += "[color=#99ffc0]It survives copying twice unchanged: genes are written in it.[/color]\n"
		"noble":
			t += "[color=#9980d9][b]Noble.[/b][/color] Its outer shell is full, so it bonds with nothing. It drifts in the sea but takes no part in life.\n"
	t += "\n[color=#789]click an element, or Left/Right\nwheel zoom   H: what am I looking at?\nTab: next scale[/color]"
	return t

## Bond strength from the selected element to every element: covalent (solid) and the weak
## pairing that copies chains (light), in kT (thermal energy at 300 K).
func _draw_bars() -> void:
	if elements.is_empty():
		return
	var font := ThemeDB.fallback_font
	var sz := bars.size
	bars.draw_rect(Rect2(Vector2.ZERO, sz), Color(0.03, 0.05, 0.08, 0.85))
	var e: Dictionary = elements[selected]
	bars.draw_string(font, Vector2(10, 16), "%s bonds with... (solid: chemical bond, light: weak pairing used in copying; kT = heat energy at 300 K)" % e.name, HORIZONTAL_ALIGNMENT_LEFT, -1, 12, Color(1, 1, 1, 0.8))
	var b: PackedFloat32Array = e.bonds
	var w: PackedFloat32Array = e.weak
	var top := 1.0
	for v in b:
		top = max(top, v)
	var n := b.size()
	var bw: float = (sz.x - 20.0) / n
	var base := sz.y - 22.0
	var h := sz.y - 52.0
	for i in n:
		var x := 10.0 + i * bw
		var other: Dictionary = elements[i]
		var col: Color = ROLE_COL[other.role]
		var bh: float = h * b[i] / top
		bars.draw_rect(Rect2(Vector2(x + 4, base - bh), Vector2(bw * 0.45 - 4, bh)), col)
		var wh: float = min(h, h * w[i] / top * 4.0)
		bars.draw_rect(Rect2(Vector2(x + bw * 0.45 + 2, base - wh), Vector2(bw * 0.3, wh)), Color(col, 0.45))
		bars.draw_string(font, Vector2(x + 4, base + 15), other.name, HORIZONTAL_ALIGNMENT_LEFT, -1, 11, Color(1, 1, 1, 0.8))
		if b[i] > 0.0:
			bars.draw_string(font, Vector2(x + 4, base - bh - 3), "%.0f" % b[i], HORIZONTAL_ALIGNMENT_LEFT, -1, 10, Color(1, 1, 1, 0.65))
