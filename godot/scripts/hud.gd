## Overlay: environment drivers, orrery, census, selection. Reads the active view.
extends CanvasLayer

var view: Node
var label: RichTextLabel
var orrery: Control
var bars := {}
var right: RichTextLabel

const SEASONS := ["spring", "summer", "autumn", "winter"]

func _ready() -> void:
	var panel := PanelContainer.new()
	panel.position = Vector2(12, 12)
	panel.custom_minimum_size = Vector2(330, 0)
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(0.03, 0.05, 0.08, 0.82)
	sb.set_corner_radius_all(8)
	sb.content_margin_left = 12
	sb.content_margin_right = 12
	sb.content_margin_top = 10
	sb.content_margin_bottom = 10
	panel.add_theme_stylebox_override("panel", sb)
	add_child(panel)
	var box := VBoxContainer.new()
	panel.add_child(box)

	label = RichTextLabel.new()
	label.bbcode_enabled = true
	label.fit_content = true
	label.scroll_active = false
	label.custom_minimum_size = Vector2(306, 0)
	box.add_child(label)

	orrery = Control.new()
	orrery.custom_minimum_size = Vector2(306, 150)
	orrery.draw.connect(_draw_orrery)
	box.add_child(orrery)

	for k in ["visible", "uv", "radio", "cosmic", "star", "tide"]:
		var row := HBoxContainer.new()
		var l := Label.new()
		l.text = k
		l.custom_minimum_size = Vector2(70, 0)
		l.add_theme_font_size_override("font_size", 13)
		row.add_child(l)
		var pb := ProgressBar.new()
		pb.custom_minimum_size = Vector2(220, 12)
		pb.show_percentage = false
		pb.max_value = 1.0
		pb.step = 0.001
		row.add_child(pb)
		box.add_child(row)
		bars[k] = pb

	var rp := PanelContainer.new()
	rp.add_theme_stylebox_override("panel", sb)
	rp.anchor_left = 1.0
	rp.anchor_right = 1.0
	rp.offset_left = -330
	rp.offset_right = -12
	rp.offset_top = 12
	add_child(rp)
	right = RichTextLabel.new()
	right.bbcode_enabled = true
	right.fit_content = true
	right.scroll_active = false
	right.custom_minimum_size = Vector2(300, 0)
	rp.add_child(right)

func _process(_d: float) -> void:
	if view == null:
		return
	var e: Dictionary = view.env
	if e.is_empty():
		return
	var day := int(floor(e.t_days))
	var mins := int(e.day_phase * 1440.0)
	var season: String = SEASONS[int(e.year_phase * 4.0) % 4]
	var tide_dir := "rising" if e.tide_rate > 0 else "falling"
	var alerts := ""
	if e.flare > 0.15:
		alerts += "[color=#ff70ff]STELLAR FLARE[/color]  "
	if e.supernova > 0.05:
		alerts += "[color=#ffffff][b]SUPERNOVA[/b][/color]"
	label.text = "[b]PRIMORDIUM[/b]  [color=#8ab]%s[/color]\n" % view.SCALE_NAME + \
		"Day %d  %02d:%02d   year %d, %s\n" % [day, mins / 60, mins % 60, int(e.year), season] + \
		"%.1f K   tide %.2f %s   cloud %.0f%%\n" % [e.temperature_k, e.tide_level, tide_dir, e.cloud * 100.0] + \
		"speed x%d   ledger drift %d\n" % [view.speed(), e.ledger_drift] + alerts
	bars.visible.value = clamp(e.visible, 0.0, 1.0)
	bars.uv.value = clamp(e.uv / 2.0, 0.0, 1.0)
	bars.radio.value = clamp(e.radio / 2.0, 0.0, 1.0)
	bars.cosmic.value = clamp(e.cosmic / 3.0, 0.0, 1.0)
	bars.star.value = clamp(e.star_activity, 0.0, 1.0)
	bars.tide.value = e.tide_level
	orrery.queue_redraw()

	if view.has_method("hud_text"):
		right.text = view.hud_text()
		return
	var c: Dictionary = view.census
	var t := "[b]Census[/b]\n"
	if not c.is_empty():
		t += "molecules %d   bonds %d\nlargest %d atoms   excited %d\n" % [c.molecules, c.bonds, c.largest, c.excited]
		t += "[color=#9ab]" + ", ".join(PackedStringArray(c.top.slice(0, 8))) + "[/color]\n"
	var s: Dictionary = view.selected_info()
	if not s.is_empty():
		t += "\n[b]Selected[/b] %s (Z=%d)\nbonds %d/%d   excite %.1f kT   weak %s\nmolecule %s (%d atoms)\n" % [
			s.name, s.z, s.bonds, s.valence, s.excite, "yes" if s.weak else "no", s.molecule, s.molecule_size]
	var h: Dictionary = view.hybrid
	if not h.is_empty():
		t += "\n[b]Resolution[/b] %s\n" % ("auto (A)" if view.auto else "manual (A)")
		if not h.has_rates:
			t += "[color=#f88]no data/rates0.json: run primordium train[/color]\n"
		t += "detail atoms %d   lifts %d   restricts %d\n" % [h.active_atoms, h.lifts, h.restricts]
		t += "[b]Discoveries[/b] %d\n" % h.discoveries
		for d in view.discoveries:
			t += "[color=#cbf]" + d + "[/color]\n"
	t += "\n[color=#789]Tab: switch scale\nwheel zoom (zoom far in for atom shells)\nright-drag pan, left-click inspect\nspace pause, 1-5 speed, A auto-resolution, R reseed[/color]"
	right.text = t

func _draw_orrery() -> void:
	var e: Dictionary = view.env
	if e.is_empty():
		return
	var c := Vector2(110, 75)
	var star_col := Color(1.0, 0.85, 0.45).lerp(Color(1, 1, 1), clamp(e.flare, 0.0, 1.0))
	orrery.draw_circle(c, 9.0 + 4.0 * e.flare, star_col)
	orrery.draw_arc(c, 52.0, 0, TAU, 64, Color(1, 1, 1, 0.15), 1.0)
	var ang: float = e.year_phase * TAU
	var pp: Vector2 = c + Vector2(cos(ang), sin(ang)) * 52.0 * e.orbit_distance
	orrery.draw_circle(pp, 5.0, Color(0.3, 0.6, 0.9))
	# day side terminator: half disc facing the star
	var to_star: Vector2 = (c - pp).normalized()
	orrery.draw_arc(pp, 5.0, to_star.angle() - PI / 2, to_star.angle() + PI / 2, 12, Color(1, 1, 0.8, 0.8), 2.0)
	for k in 2:
		var mph: float = e["moon%d_phase" % k] * TAU
		var r := 13.0 + 7.0 * k
		orrery.draw_arc(pp, r, 0, TAU, 32, Color(1, 1, 1, 0.08), 1.0)
		orrery.draw_circle(pp + Vector2(cos(mph), sin(mph)) * r, 2.2 - 0.6 * k, Color(0.85, 0.85, 0.8))
	# far sources: pulsar blink, binary, supernova flash
	var pulse: float = clamp((e.radio - 0.4) * 2.0, 0.0, 1.0)
	orrery.draw_circle(Vector2(250, 30), 2.0 + 3.0 * pulse, Color(0.6, 0.8, 1.0, 0.4 + 0.6 * pulse))
	orrery.draw_string(ThemeDB.fallback_font, Vector2(232, 52), "pulsar", HORIZONTAL_ALIGNMENT_LEFT, -1, 10, Color(1, 1, 1, 0.5))
	if e.supernova > 0.02:
		orrery.draw_circle(Vector2(260, 110), 3.0 + 25.0 * clamp(e.supernova, 0.0, 1.0), Color(1, 1, 1, 0.5))
	orrery.draw_string(ThemeDB.fallback_font, Vector2(4, 146), "year %.2f  dist %.3f AU" % [e.year_phase, e.orbit_distance], HORIZONTAL_ALIGNMENT_LEFT, -1, 11, Color(1, 1, 1, 0.6))
