## Close-up of one chain: fold with letters and catalytic pockets; click a residue for its atoms.
## Self-contained: give it `info` (chain dictionary from a bridge) and a size.
extends Control

## Residue letter -> element Z, from the table.
var LETTER_Z: PackedByteArray = ElementsSim.letters_z()
var LETTER_NAMES: PackedStringArray = ElementsSim.letter_names()
var NAMES: PackedStringArray = ElementsSim.element_names()
var LETTER_CHARS: String = ElementsSim.letter_chars()

var info := {}
var residue := -1
var fold_h := 230.0
var palette: Array = []

func _ready() -> void:
	palette = load("res://scripts/chem_view.gd").palette()
	mouse_filter = Control.MOUSE_FILTER_STOP
	gui_input.connect(_on_input)

func show_chain(d: Dictionary) -> void:
	if d.is_empty() or info.is_empty() or d.id != info.id:
		residue = -1
	info = d
	if residue >= 0 and not info.is_empty():
		residue = min(residue, info.letters.size() - 1)
	queue_redraw()

func _process(_d: float) -> void:
	if residue >= 0 and visible:
		queue_redraw()

func _layout() -> Dictionary:
	var fold: PackedVector2Array = info.fold
	var lo := Vector2(1e9, 1e9)
	var hi := Vector2(-1e9, -1e9)
	for p in fold:
		lo = lo.min(p)
		hi = hi.max(p)
	var span: Vector2 = (hi - lo) + Vector2.ONE
	var box := Rect2(10, 26, size.x - 20, fold_h - 30)
	var s: float = min(min(box.size.x / span.x, box.size.y / span.y), 34.0)
	var off: Vector2 = box.position + (box.size - span * s) * 0.5 - lo * s + Vector2(s, s) * 0.5
	return {"s": s, "off": off}

## Residue atoms from the table (PoolSim.residue_atoms): backbone, side atom, Hy caps.
func residue_atoms(letter: int) -> Array:
	return PoolSim.residue_atoms(letter)

## Backbone Z: the first atom of any residue.
var backbone_z: int = int(PoolSim.residue_atoms(0)[0].z)

func _on_input(e: InputEvent) -> void:
	if not (e is InputEventMouseButton and e.pressed and e.button_index == MOUSE_BUTTON_LEFT) or info.is_empty():
		return
	var lay := _layout()
	var fold: PackedVector2Array = info.fold
	for k in fold.size():
		if (fold[k] * lay.s + lay.off).distance_to(e.position) < lay.s * 0.5:
			residue = k
			queue_redraw()
			accept_event()
			return

func _draw() -> void:
	if info.is_empty():
		return
	var font := ThemeDB.fallback_font
	draw_rect(Rect2(Vector2.ZERO, size), Color(0.03, 0.05, 0.08, 0.86))
	draw_string(font, Vector2(10, 18), "Chain %d fold (dashes = catalytic pockets)" % info.id, HORIZONTAL_ALIGNMENT_LEFT, -1, 12, Color(1, 1, 1, 0.8))
	var lay := _layout()
	var fold: PackedVector2Array = info.fold
	var letters: PackedByteArray = info.letters
	var s: float = lay.s
	for k in fold.size() - 1:
		draw_line(fold[k] * s + lay.off, fold[k + 1] * s + lay.off, Color(0.9, 0.9, 0.85, 0.8), 3.0)
	var sites: PackedVector2Array = info.sites
	for k in range(0, sites.size(), 2):
		draw_dashed_line(sites[k] * s + lay.off, sites[k + 1] * s + lay.off, Color(1.0, 0.85, 0.3, 0.9), 2.0, 4.0)
	for k in fold.size():
		var c: Vector2 = fold[k] * s + lay.off
		draw_circle(c, s * 0.32, palette[LETTER_Z[letters[k]]])
		if k == residue:
			draw_arc(c, s * 0.45, 0, TAU, 24, Color(1, 1, 0.4), 2.0)
		draw_string(font, c + Vector2(-4, 4), LETTER_CHARS[letters[k]], HORIZONTAL_ALIGNMENT_LEFT, -1, 11, Color(0, 0, 0, 0.8))
	var y0 := fold_h + 6.0
	if residue < 0 or residue >= letters.size():
		draw_string(font, Vector2(10, y0), "click a residue to see its atoms", HORIZONTAL_ALIGNMENT_LEFT, -1, 12, Color(1, 1, 1, 0.5))
		return
	var letter: int = letters[residue]
	draw_string(font, Vector2(10, y0), "Residue %d: %s backbone + %s side group" % [residue, NAMES[backbone_z], LETTER_NAMES[letter]], HORIZONTAL_ALIGNMENT_LEFT, -1, 12, Color(1, 1, 1, 0.8))
	var atoms := residue_atoms(letter)
	var spacing: float = min(62.0, (size.y - y0 - 40.0) / 2.2)
	var origin := Vector2(size.x * 0.5, y0 + 34.0)
	var t := Time.get_ticks_msec() / 1000.0
	for k in range(1, atoms.size()):
		# the side atom bonds to the backbone, every cap to the side atom
		var parent: int = 0 if k == 1 else 1
		draw_line(origin + atoms[parent].pos * spacing, origin + atoms[k].pos * spacing, Color(1, 1, 1, 0.7), 3.0)
	for k in atoms.size():
		var a: Dictionary = atoms[k]
		var c: Vector2 = origin + a.pos * spacing
		var z: int = a.z
		var col: Color = palette[z]
		draw_circle(c, 24, Color(col, 0.18))
		draw_circle(c, 5, col.lerp(Color.WHITE, 0.4))
		draw_arc(c, 11, 0, TAU, 24, Color(col, 0.6), 1.0)
		for e in min(z, 2):
			var ang: float = t * 2.3 + PI * e + k
			draw_circle(c + Vector2(cos(ang), sin(ang)) * 11, 2.2, Color(0.6, 0.85, 1.0))
		var outer: int = max(z - 2, 0)
		if outer > 0:
			draw_arc(c, 20, 0, TAU, 32, Color(col, 0.6), 1.0)
		for e in outer:
			var ang: float = -t * 1.1 + TAU * e / outer + k
			draw_circle(c + Vector2(cos(ang), sin(ang)) * 20, 2.6, Color.WHITE if e < a.bonds else Color(0.55, 0.8, 1.0))
		draw_string(font, c + Vector2(28, 5), "%s (Z=%d)" % [NAMES[z], z], HORIZONTAL_ALIGNMENT_LEFT, -1, 12, Color(1, 1, 1, 0.75))
