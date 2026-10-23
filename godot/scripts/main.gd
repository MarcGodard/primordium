## Root: the hub, one scale view at a time, the HUD, a scale bar, warp transitions and the
## campaign (goals, codex, kept creatures). Starts in the hub unless --scale=K is given.
## Capture mode (no interaction, exits): godot --path godot -- --shot=out.png [--scale=K] [--warm=SWEEPS] [--zoom=Z] [--focus=X,Y] [--frames=N]
extends Node

var view: Node
var views: Array = []
var hud: CanvasLayer
var hub: Node3D
var in_hub := true
var shot := ""
var frames := 20
var bar: HBoxContainer
var bar_buttons: Array = []
var warp: ColorRect
var warp_title: Label
var warp_sub: Label
var travelling := false
## Captures normally skip the warp; --warp=K plays it so a frame mid-transition can be checked.
var show_warp := false
var campaign: Node
var goal_label: RichTextLabel
var goal_bar: ProgressBar
var goal_panel: PanelContainer
var codex_panel: PanelContainer
var codex_text: RichTextLabel
var gallery: SubViewportContainer
var gallery_caption: RichTextLabel
var gallery_list: ItemList
var toast_label: Label
var toast_tw: Tween
var card: PanelContainer
var card_title: Label
var card_body: RichTextLabel
var card_hint: Label
## World the graduation card leads to on Enter (-1: just close).
var card_next := -1

func _ready() -> void:
	campaign = load("res://scripts/campaign.gd").new()
	add_child(campaign)
	# the garden is a second reef, empty until the player releases kept creatures into it
	for path in ["res://scripts/periodic_view.gd", "res://scripts/chem_view.gd", "res://scripts/pool_view.gd", "res://scripts/pond_view.gd", "res://scripts/cell_view.gd", "res://scripts/coast_view.gd", "res://scripts/reef_view.gd", "res://scripts/reef_view.gd"]:
		var v: Node = load(path).new()
		if views.size() == 7:
			v.garden = true
		if "campaign" in v:
			v.campaign = campaign
		add_child(v)
		views.append(v)
	view = views[0]
	for v in views:
		v.activate(false)
	hud = load("res://scripts/hud.gd").new()
	hud.view = view
	add_child(hud)
	hub = load("res://scripts/hub.gd").new()
	hub.campaign = campaign
	add_child(hub)
	hub.chosen.connect(travel)
	_build_bar()
	_build_warp()
	_build_campaign_ui()
	campaign.toast.connect(show_toast)
	campaign.graduated.connect(show_graduation)
	enter_hub()
	# a new player starts at the smallest scale; the hub comes once there is somewhere to go
	var capture := false
	for a in OS.get_cmdline_user_args():
		capture = capture or a.begins_with("--shot=")
	if campaign.is_new() and not capture:
		welcome()
	for a in OS.get_cmdline_user_args():
		var kv: PackedStringArray = a.trim_prefix("--").split("=")
		if kv.size() != 2:
			continue
		match kv[0]:
			"shot":
				shot = kv[1]
				# captures must not steal focus or eat the user's keystrokes
				DisplayServer.window_set_flag(DisplayServer.WINDOW_FLAG_NO_FOCUS, true)
				view.set_process_unhandled_input(false)
				hub.set_process_unhandled_input(false)
				hub.hover_enabled = false
			"frames": frames = int(kv[1])
			"welcome": welcome()
			"graduate": campaign.complete(int(kv[1]), "")
			"explore": campaign.state.explore = kv[1] == "1"
			"codex":
				codex_panel.visible = kv[1] == "1"
				_fill_codex()
			"scale": switch_to(int(kv[1]))
			"pick": view.pick_longest()
			"run":
				# reef: restart as run N (seed N)
				if view.has_method("new_run"):
					campaign.state.run = int(kv[1]) - 1
					view.new_run()
			"hunt":
				# reef: run on (up to N steps) until a creature exists, then select it
				if view.has_method("hunt"):
					view.hunt(int(kv[1]))
			"ff":
				if view.has_method("ff_wait"):
					view.ff_wait(float(kv[1]))
			"levers":
				# open the view's lever panel; "key:mult" also sets one
				var kv3: PackedStringArray = kv[1].split(":")
				if view.get("levers") is Control:
					view.levers_open = true
					view.levers.visible = true
					if kv3.size() == 2:
						view.sim.set_lever(kv3[0], float(kv3[1]))
					view.levers.refresh()
				if view.get("lever_panel") != null:
					view.lever_panel.visible = true
					var kv2: PackedStringArray = kv[1].split(":")
					if kv2.size() == 2:
						view._set_lever(kv2[0], float(kv2[1]))
					view._show_levers()
			"skins":
				if "skin_all" in view:
					view.skin_all = kv[1] == "1"
			"keep":
				if view.has_method("keep_selected"):
					view.keep_selected()
			"explain": hud.explaining = kv[1] == "1"
			"warp":
				show_warp = true
				travel(int(kv[1]))
			"view3d":
				if view.has_method("set_3d"):
					view.set_3d(kv[1] == "1")
			"lineage":
				if view.has_method("set_lineage"):
					view.set_lineage(kv[1] == "1")
			"warm": view.warm(int(kv[1]))
			"zoom":
				if view.cam is Camera2D:
					view.cam.zoom = Vector2(float(kv[1]), float(kv[1]))
				else:
					view.dist = float(kv[1])
			"focus":
				var xy := kv[1].split(",")
				if view.cam is Camera2D:
					view.cam.position = Vector2(float(xy[0]), float(xy[1])) * view.CELL
				else:
					view.target = Vector3(float(xy[0]), view.target.y, -float(xy[1]))

## Show scale k now, no transition.
func switch_to(k: int) -> void:
	view = views[posmod(k, views.size())]
	in_hub = false
	hub.activate(false)
	for v in views:
		v.activate(v == view)
	hud.view = view
	hud.visible = true
	hud.set_process(true)
	hud.set_process_unhandled_input(true)
	bar.visible = true
	_color_bar()

## Current world in its colour, open ones grey, locked ones dim.
func _color_bar() -> void:
	for i in bar_buttons.size():
		var b: Button = bar_buttons[i]
		var here: bool = not in_hub and views[i] == view
		var open: bool = campaign.is_open(i)
		b.add_theme_color_override("font_color", hub.SCALES[i][2] if here else (Color(0.55, 0.62, 0.72) if open else Color(0.3, 0.33, 0.4)))
		b.add_theme_font_size_override("font_size", 14 if here else 12)

func enter_hub() -> void:
	in_hub = true
	for v in views:
		v.activate(false)
	hub.activate(true)
	# the HUD and its guide belong to the scales: off entirely in the hub (its guide sits on
	# its own layer, so hiding the HUD alone would not hide it)
	hud.visible = false
	hud.explaining = false
	hud.explain_panel.visible = false
	hud.set_process(false)
	hud.set_process_unhandled_input(false)
	bar.visible = false

## Travel to scale k (or the hub when k < 0) through a warp: streaks rush outward when
## climbing to a bigger scale, inward when diving smaller or in from the hub.
func travel(k: int) -> void:
	if travelling:
		return
	var from := -1 if in_hub else views.find(view)
	if k == from:
		return
	if k >= 0 and not campaign.is_open(k) and shot == "":
		var why: String = "Keep a creature at the reef (K) to open your world." if k == 7 else "Reach the goal of %s first: %s." % [hub.SCALES[k - 1][0], campaign.goal_title(k - 1)]
		show_toast("Locked: " + hub.SCALES[k][0], why, Color(0.7, 0.75, 0.85))
		return
	if shot != "" and not show_warp:
		if k < 0:
			enter_hub()
		else:
			switch_to(k)
		return
	travelling = true
	var outward := k >= 0 and from >= 0 and k > from
	var m: ShaderMaterial = warp.material
	m.set_shader_parameter("dir", 1.0 if outward else -1.0)
	m.set_shader_parameter("tint", hub.SCALES[k][2] if k >= 0 else Color(0.6, 0.7, 1.0))
	warp_title.text = "The hub" if k < 0 else (hub.SCALES[k][0] if k == hub.GARDEN else "Scale %d  ·  %s" % [k - 1, hub.SCALES[k][0]])
	warp_title.add_theme_color_override("font_color", hub.SCALES[k][2] if k >= 0 else Color(0.8, 0.9, 1.0))
	warp_sub.text = hub.SCALES[k][1] if k >= 0 else "Seven scales of one world."
	warp.visible = true
	var tw := create_tween()
	tw.tween_method(func(p: float) -> void: m.set_shader_parameter("p", p), 0.0, 1.0, 0.45).set_ease(Tween.EASE_IN)
	tw.parallel().tween_property(warp_title, "modulate:a", 1.0, 0.45)
	tw.parallel().tween_property(warp_sub, "modulate:a", 1.0, 0.45)
	tw.tween_callback(func() -> void:
		if k < 0:
			enter_hub()
		else:
			switch_to(k))
	tw.tween_interval(0.35)
	tw.tween_method(func(p: float) -> void: m.set_shader_parameter("p", p), 1.0, 0.0, 0.6).set_ease(Tween.EASE_OUT)
	tw.parallel().tween_property(warp_title, "modulate:a", 0.0, 0.8)
	tw.parallel().tween_property(warp_sub, "modulate:a", 0.0, 0.8)
	tw.tween_callback(func() -> void:
		warp.visible = false
		travelling = false)

func _build_bar() -> void:
	var layer := CanvasLayer.new()
	layer.layer = 9
	add_child(layer)
	bar = HBoxContainer.new()
	bar.anchor_left = 0.5
	bar.anchor_right = 0.5
	bar.offset_left = -440
	bar.offset_right = 440
	bar.offset_top = 4
	bar.alignment = BoxContainer.ALIGNMENT_CENTER
	bar.add_theme_constant_override("separation", 2)
	layer.add_child(bar)
	var home := Button.new()
	home.text = "◉ hub"
	home.flat = true
	home.focus_mode = Control.FOCUS_NONE
	home.add_theme_font_size_override("font_size", 12)
	home.pressed.connect(func() -> void: travel(-1))
	bar.add_child(home)
	for k in hub.SCALES.size():
		var b := Button.new()
		b.text = ("· " if k > 0 else "") + hub.label(k).replace("  ", " ")
		b.flat = true
		b.focus_mode = Control.FOCUS_NONE
		b.pressed.connect(func() -> void: travel(k))
		bar.add_child(b)
		bar_buttons.append(b)

func _build_warp() -> void:
	var layer := CanvasLayer.new()
	layer.layer = 20
	add_child(layer)
	warp = ColorRect.new()
	warp.set_anchors_preset(Control.PRESET_FULL_RECT)
	warp.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var m := ShaderMaterial.new()
	m.shader = load("res://shaders/warp.gdshader")
	warp.material = m
	warp.visible = false
	layer.add_child(warp)
	warp_title = Label.new()
	warp_title.add_theme_font_size_override("font_size", 44)
	warp_title.set_anchors_preset(Control.PRESET_CENTER)
	warp_title.offset_left = -500
	warp_title.offset_right = 500
	warp_title.offset_top = -40
	warp_title.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	warp_title.modulate.a = 0.0
	warp.add_child(warp_title)
	warp_sub = Label.new()
	warp_sub.add_theme_font_size_override("font_size", 18)
	warp_sub.add_theme_color_override("font_color", Color(0.8, 0.86, 0.95))
	warp_sub.set_anchors_preset(Control.PRESET_CENTER)
	warp_sub.offset_left = -500
	warp_sub.offset_right = 500
	warp_sub.offset_top = 20
	warp_sub.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	warp_sub.modulate.a = 0.0
	warp.add_child(warp_sub)

func _unhandled_input(e: InputEvent) -> void:
	if not (e is InputEventKey and e.pressed):
		return
	if e.keycode == KEY_J:
		codex_panel.visible = not codex_panel.visible
		if codex_panel.visible:
			_fill_codex()
	elif e.keycode == KEY_ESCAPE and codex_panel.visible:
		codex_panel.visible = false
	elif e.keycode == KEY_M:
		travel(-1)
	elif e.keycode == KEY_TAB:
		if in_hub:
			travel(views.size() - 1 if e.shift_pressed else 0)
		else:
			# step past locked worlds
			var step := -1 if e.shift_pressed else 1
			var k := posmod(views.find(view) + step, views.size())
			while not campaign.is_open(k) and k != views.find(view):
				k = posmod(k + step, views.size())
			travel(k)

func _process(d: float) -> void:
	_update_goal(d)
	if shot == "":
		return
	frames -= 1
	if frames == 0:
		if not in_hub:
			print("goal: ", campaign.progress)
		for v in views:
			if v.has_method("save_garden"):
				v.save_garden()
		var img := get_viewport().get_texture().get_image()
		img.save_png(shot)
		print("shot: ", shot)
		get_tree().quit()

func _panel_style(alpha: float) -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(0.02, 0.04, 0.07, alpha)
	sb.border_color = Color(0.95, 0.8, 0.4, 0.45)
	sb.set_border_width_all(1)
	sb.set_corner_radius_all(8)
	sb.content_margin_left = 18
	sb.content_margin_right = 18
	sb.content_margin_top = 12
	sb.content_margin_bottom = 12
	return sb

## Goal banner under the scale bar, codex page (J) and toasts.
func _build_campaign_ui() -> void:
	var layer := CanvasLayer.new()
	layer.layer = 11
	add_child(layer)
	var gp := PanelContainer.new()
	var gsb := _panel_style(0.78)
	gsb.border_color = Color(1, 1, 1, 0.08)
	gsb.content_margin_top = 6
	gsb.content_margin_bottom = 8
	gp.add_theme_stylebox_override("panel", gsb)
	gp.anchor_left = 0.5
	gp.anchor_right = 0.5
	gp.offset_left = -350
	gp.offset_right = 350
	gp.offset_top = 32
	gp.mouse_filter = Control.MOUSE_FILTER_IGNORE
	layer.add_child(gp)
	goal_panel = gp
	var box := VBoxContainer.new()
	box.mouse_filter = Control.MOUSE_FILTER_IGNORE
	gp.add_child(box)
	goal_label = RichTextLabel.new()
	goal_label.bbcode_enabled = true
	goal_label.fit_content = true
	goal_label.scroll_active = false
	goal_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	goal_label.add_theme_font_size_override("normal_font_size", 13)
	goal_label.add_theme_font_size_override("bold_font_size", 14)
	box.add_child(goal_label)
	goal_bar = ProgressBar.new()
	goal_bar.custom_minimum_size = Vector2(660, 5)
	goal_bar.show_percentage = false
	goal_bar.max_value = 1.0
	goal_bar.step = 0.001
	var fill := StyleBoxFlat.new()
	fill.bg_color = Color(1.0, 0.8, 0.35)
	goal_bar.add_theme_stylebox_override("fill", fill)
	var bg := StyleBoxFlat.new()
	bg.bg_color = Color(1, 1, 1, 0.08)
	goal_bar.add_theme_stylebox_override("background", bg)
	box.add_child(goal_bar)
	codex_panel = PanelContainer.new()
	codex_panel.add_theme_stylebox_override("panel", _panel_style(0.98))
	codex_panel.anchor_left = 0.5
	codex_panel.anchor_right = 0.5
	codex_panel.anchor_top = 0.5
	codex_panel.anchor_bottom = 0.5
	codex_panel.offset_left = -620
	codex_panel.offset_right = 620
	codex_panel.offset_top = -340
	codex_panel.offset_bottom = 340
	var cols := HBoxContainer.new()
	cols.add_theme_constant_override("separation", 18)
	codex_panel.add_child(cols)
	codex_text = RichTextLabel.new()
	codex_text.bbcode_enabled = true
	codex_text.scroll_active = true
	codex_text.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	codex_text.add_theme_font_size_override("normal_font_size", 15)
	codex_text.add_theme_font_size_override("bold_font_size", 16)
	cols.add_child(codex_text)
	# the collection: kept creatures, the selected one in 3D
	var right := VBoxContainer.new()
	right.custom_minimum_size = Vector2(420, 0)
	cols.add_child(right)
	var head := Label.new()
	head.text = "Your creatures"
	head.add_theme_font_size_override("font_size", 18)
	head.add_theme_color_override("font_color", Color(0.75, 1.0, 0.55))
	right.add_child(head)
	gallery = load("res://scripts/gallery.gd").new()
	gallery.custom_minimum_size = Vector2(420, 330)
	right.add_child(gallery)
	gallery_caption = RichTextLabel.new()
	gallery_caption.bbcode_enabled = true
	gallery_caption.fit_content = true
	gallery_caption.scroll_active = false
	gallery_caption.add_theme_font_size_override("normal_font_size", 14)
	right.add_child(gallery_caption)
	gallery_list = ItemList.new()
	gallery_list.size_flags_vertical = Control.SIZE_EXPAND_FILL
	gallery_list.focus_mode = Control.FOCUS_NONE
	gallery_list.item_selected.connect(_show_creature)
	right.add_child(gallery_list)
	codex_panel.visible = false
	layer.add_child(codex_panel)
	toast_label = Label.new()
	toast_label.anchor_left = 0.5
	toast_label.anchor_right = 0.5
	toast_label.anchor_top = 1.0
	toast_label.anchor_bottom = 1.0
	toast_label.offset_left = -450
	toast_label.offset_right = 450
	toast_label.offset_top = -230
	toast_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	toast_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	toast_label.add_theme_font_size_override("font_size", 20)
	toast_label.add_theme_color_override("font_outline_color", Color(0, 0, 0, 0.9))
	toast_label.add_theme_constant_override("outline_size", 6)
	toast_label.modulate.a = 0.0
	# above the fast-forward report (12), which may be open when a toast fires
	var tl := CanvasLayer.new()
	tl.layer = 13
	add_child(tl)
	tl.add_child(toast_label)
	_build_card(tl)
	campaign.changed.connect(func() -> void:
		_color_bar()
		if codex_panel.visible:
			_fill_codex())

func _build_card(parent: CanvasLayer) -> void:
	card = PanelContainer.new()
	var sb := _panel_style(0.96)
	sb.border_color = Color(1.0, 0.82, 0.4, 0.8)
	sb.set_border_width_all(2)
	sb.content_margin_left = 30
	sb.content_margin_right = 30
	sb.content_margin_top = 22
	sb.content_margin_bottom = 22
	card.add_theme_stylebox_override("panel", sb)
	card.set_anchors_preset(Control.PRESET_CENTER)
	card.offset_left = -380
	card.offset_right = 380
	card.offset_top = -210
	card.offset_bottom = 210
	var box := VBoxContainer.new()
	box.add_theme_constant_override("separation", 14)
	card.add_child(box)
	card_title = Label.new()
	card_title.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	card_title.add_theme_font_size_override("font_size", 30)
	card_title.add_theme_color_override("font_color", Color(1.0, 0.85, 0.45))
	box.add_child(card_title)
	card_body = RichTextLabel.new()
	card_body.bbcode_enabled = true
	card_body.fit_content = true
	card_body.scroll_active = false
	card_body.add_theme_font_size_override("normal_font_size", 16)
	card_body.add_theme_font_size_override("bold_font_size", 17)
	box.add_child(card_body)
	card_hint = Label.new()
	card_hint.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	card_hint.add_theme_font_size_override("font_size", 17)
	card_hint.add_theme_color_override("font_color", Color(0.75, 0.85, 1.0))
	box.add_child(card_hint)
	card.visible = false
	parent.add_child(card)

## New players: start at the smallest scale with a word on what the game is.
func welcome() -> void:
	switch_to(0)
	show_card("Welcome to Primordium", "Everything alive is built from atoms, and every atom here comes from one rule about electron shells. You start at the smallest scale. Reach each scale's goal (top of the screen) to graduate to the next: molecules, chains that copy themselves, cells, a coastline of life, and finally many-celled creatures you can breed in a world of your own.\n\nH explains any screen. J opens your codex.", "Enter: begin", -1)

func show_card(title: String, body: String, hint: String, next: int) -> void:
	card_title.text = title
	card_body.text = body
	card_hint.text = hint
	card_next = next
	card.visible = true
	# fit the height to the text once it has laid out
	await get_tree().process_frame
	var h: float = card.get_combined_minimum_size().y
	card.offset_top = -h * 0.5
	card.offset_bottom = h * 0.5

## Goal k reached: what it means (the codex page), and the way on.
func show_graduation(k: int, page: String) -> void:
	var last: bool = k + 1 >= campaign.GOALS.size()
	var body := page + "\n\n"
	if last:
		body += "[b]The campaign is complete.[/b] Explore mode is open: E in the hub opens every world. Your world keeps evolving whenever you come back."
		show_card("You bred a new kind of animal", body, "Enter: continue", -1)
		return
	var nk := k + 1
	body += "[b]Next: %s.[/b] %s\n[color=#ffd36b]Goal there: %s[/color]" % [hub.SCALES[nk][0], hub.SCALES[nk][1], campaign.goal_title(nk)]
	var here: bool = not in_hub and views.find(view) == nk
	show_card("Goal reached: " + campaign.goal_title(k), body, "Enter: continue" if here else "Enter: go to %s now      Esc: stay here a while" % hub.SCALES[nk][0], -1 if here else nk)

func _input(e: InputEvent) -> void:
	if not card.visible or not (e is InputEventKey and e.pressed and not e.echo):
		return
	if e.keycode in [KEY_ENTER, KEY_KP_ENTER, KEY_SPACE]:
		card.visible = false
		if card_next >= 0:
			travel(card_next)
	elif e.keycode == KEY_ESCAPE:
		card.visible = false
	get_viewport().set_input_as_handled()

func show_toast(title: String, body: String, color: Color) -> void:
	toast_label.text = title + "\n" + body
	toast_label.add_theme_color_override("font_color", color)
	if toast_tw:
		toast_tw.kill()
	toast_label.modulate.a = 1.0
	toast_tw = create_tween()
	toast_tw.tween_interval(3.5)
	toast_tw.tween_property(toast_label, "modulate:a", 0.0, 1.0)

func _fill_codex() -> void:
	var st: Dictionary = campaign.state
	var t := "[b]Codex[/b]   [color=#9ab]run %d   goals %d of %d   J or Esc to close[/color]\n\n" % [st.run, st.done.size(), campaign.GOALS.size()]
	if st.codex.is_empty():
		t += "[color=#9ab]Empty. Each goal you reach writes a page here, and so does each creature you keep.[/color]\n"
	for c in st.codex:
		t += c.text + "\n\n"
	codex_text.text = t
	var keep_pick: int = gallery_list.get_selected_items()[0] if gallery_list.is_anything_selected() else -1
	gallery_list.clear()
	for c in st.kept:
		var label := "%s%s   %d cells, %d jobs" % ["* " if c.creature else "", c.name, c.cells, c.jobs]
		gallery_list.add_item(label.replace(", 1 jobs", ", 1 job"))
	if st.kept.is_empty():
		gallery.show_shape([])
		gallery_caption.text = "[color=#9ab]Nothing kept yet. At the reef, click a body and press K: it joins your collection and your world.[/color]"
	else:
		# keep the player's pick across refreshes; otherwise show the newest
		var i: int = keep_pick if keep_pick >= 0 and keep_pick < st.kept.size() else st.kept.size() - 1
		gallery_list.select(i)
		_show_creature(i)

func _show_creature(i: int) -> void:
	var c: Dictionary = campaign.state.kept[i]
	gallery.show_shape(c.get("shape", []))
	var fate := "lives in your world" if c.get("released", false) else "waiting to be released in your world"
	gallery_caption.text = ("[i]%s[/i]   " % c.species if c.has("species") else "") + "[b]%s[/b]%s\nfrom %s: %d cells doing %d jobs. %s.%s" % [c.name, "   [color=#ffd36b]creature[/color]" if c.creature else "", c.get("from", "a reef run"), c.cells, c.jobs, fate.capitalize().substr(0, 1) + fate.substr(1),
		"\n[color=#9ab]Colour: what a cell does more than the rest of its body: [color=#40ccd9]copying[/color] [color=#59cc4d]metabolism[/color] [color=#f2e0b8]membrane[/color] [color=#bf3852]pigment[/color] [color=#ffbf33]glue[/color][/color]" if not c.get("shape", []).is_empty() else "\n[color=#9ab](no body shape recorded for this one)[/color]"]

func _update_goal(delta: float) -> void:
	var k := -1 if in_hub else views.find(view)
	goal_panel.visible = k >= 0
	if k < 0:
		return
	if not travelling:
		campaign.tick(delta, k, view)
	var p: Dictionary = campaign.progress.get(k, {})
	var done: bool = campaign.is_done(k)
	var head := "[color=#7f7]done[/color]" if done else "[color=#ffd36b]goal[/color]"
	goal_label.text = "[center]%s  [b]%s[/b]  [color=#cde]%s[/color]\n[color=#9ab]%s[/color][/center]" % [head, campaign.goal_title(k), campaign.goal_text(k), p.get("now", "")]
	goal_bar.value = 1.0 if done else p.get("p", 0.0)
