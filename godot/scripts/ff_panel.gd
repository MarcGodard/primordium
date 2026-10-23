## Fast-forward overlay: a progress card while the reef runs on a worker thread, then the
## report of what changed. Any key or click closes the report.
extends CanvasLayer

var card: PanelContainer
var title: Label
var bar: ProgressBar
var report_panel: PanelContainer
var report: RichTextLabel

func _ready() -> void:
	layer = 12
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(0.02, 0.04, 0.07, 0.92)
	sb.border_color = Color(1.0, 0.8, 0.4, 0.5)
	sb.set_border_width_all(1)
	sb.set_corner_radius_all(8)
	sb.content_margin_left = 20
	sb.content_margin_right = 20
	sb.content_margin_top = 14
	sb.content_margin_bottom = 14
	card = PanelContainer.new()
	card.add_theme_stylebox_override("panel", sb)
	card.set_anchors_preset(Control.PRESET_CENTER)
	card.offset_left = -260
	card.offset_right = 260
	card.offset_top = -36
	card.offset_bottom = 36
	var box := VBoxContainer.new()
	card.add_child(box)
	title = Label.new()
	title.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	title.add_theme_font_size_override("font_size", 18)
	box.add_child(title)
	bar = ProgressBar.new()
	bar.max_value = 1.0
	bar.step = 0.001
	bar.show_percentage = false
	bar.custom_minimum_size = Vector2(460, 10)
	var fill := StyleBoxFlat.new()
	fill.bg_color = Color(1.0, 0.8, 0.35)
	bar.add_theme_stylebox_override("fill", fill)
	box.add_child(bar)
	card.visible = false
	add_child(card)
	report_panel = PanelContainer.new()
	report_panel.add_theme_stylebox_override("panel", sb)
	report_panel.set_anchors_preset(Control.PRESET_CENTER)
	report_panel.offset_left = -380
	report_panel.offset_right = 380
	report_panel.offset_top = -300
	report_panel.offset_bottom = 300
	report = RichTextLabel.new()
	report.bbcode_enabled = true
	report.scroll_active = true
	report.add_theme_font_size_override("normal_font_size", 15)
	report.add_theme_font_size_override("bold_font_size", 16)
	report_panel.add_child(report)
	report_panel.visible = false
	add_child(report_panel)

func show_progress(text: String, p: float) -> void:
	report_panel.visible = false
	card.visible = true
	title.text = text
	bar.value = p

func show_report(bbcode: String) -> void:
	card.visible = false
	report.text = bbcode
	report_panel.visible = true

func close() -> void:
	card.visible = false
	report_panel.visible = false

func showing_report() -> bool:
	return report_panel.visible

func _unhandled_input(e: InputEvent) -> void:
	if report_panel.visible and ((e is InputEventKey and e.pressed) or (e is InputEventMouseButton and e.pressed and e.button_index == MOUSE_BUTTON_LEFT)):
		report_panel.visible = false
		get_viewport().set_input_as_handled()
