## Which periodic table the game runs on. Table fixed at compile time (core feature), so each
## table is its own native build; this autoload swaps the library in before any script that
## names a native class compiles. Must stay free of native class names itself.
## Choice saved in user://table.txt; --table=KEY overrides it for one run (captures).
extends Node

const DEFAULT := "normal"
## key, shells, label, extension config (empty: the default primordium.gdextension), cargo build line.
const TABLES := [
	["normal", [2, 6], "Normal", "", "cargo build --release -p primordium-godot"],
	["hard", [2, 6, 6], "Hard", "res://tables/hard.cfg", "CARGO_TARGET_DIR=target/t266 cargo build --release -p primordium-godot --features shells-266"],
	["very_hard", [2, 6, 6, 6], "Very hard", "res://tables/very_hard.cfg", "CARGO_TARGET_DIR=target/t2666 cargo build --release -p primordium-godot --features shells-2666"],
]
const CHOICE := "user://table.txt"

var key := DEFAULT
## Set when the chosen table's build is missing: the game fell back to Normal.
var missing := ""

func _init() -> void:
	var want := DEFAULT
	var args := OS.get_cmdline_user_args()
	# captures stay on Normal unless they ask: the player's choice must not change a capture
	if FileAccess.file_exists(CHOICE) and not Array(args).any(func(a: String) -> bool: return a.begins_with("--shot=")):
		want = FileAccess.get_file_as_string(CHOICE).strip_edges()
	for a in args:
		if a.begins_with("--table="):
			want = a.trim_prefix("--table=")
	var t := find(want)
	if t.is_empty() or t[3] == "":
		return
	if not FileAccess.file_exists(_lib(t[3])):
		missing = "%s needs its build: %s" % [t[2], t[4]]
		# back to Normal on disk too: else the menu (on Normal) can never undo the choice
		_save(DEFAULT)
		return
	GDExtensionManager.unload_extension("res://primordium.gdextension")
	if GDExtensionManager.load_extension(t[3]) != GDExtensionManager.LOAD_STATUS_OK:
		GDExtensionManager.load_extension("res://primordium.gdextension")
		missing = "%s failed to load: %s" % [t[2], t[3]]
		return
	key = want

func find(k: String) -> Array:
	for t in TABLES:
		if t[0] == k:
			return t
	return []

func current() -> Array:
	return find(key)

## Table for a shells list, or [] if that table is not playable.
func for_shells(shells: Array) -> Array:
	for t in TABLES:
		if t[1] == shells:
			return t
	return []

## Campaign save per table, so switching never mixes progress.
func save_path() -> String:
	return "user://campaign.json" if key == DEFAULT else "user://campaign-%s.json" % key

## Save the choice and restart on it.
func switch(k: String) -> void:
	_save(k)
	# get_cmdline_args drops --path: without it the restart opens no project
	var args := ["--path", ProjectSettings.globalize_path("res://")]
	args.append_array(OS.get_cmdline_args())
	var user := Array(OS.get_cmdline_user_args()).filter(func(a: String) -> bool: return not a.begins_with("--table="))
	if not user.is_empty():
		args.append("--")
		args.append_array(user)
	OS.set_restart_on_exit(true, PackedStringArray(args))
	get_tree().quit()

func _save(k: String) -> void:
	var f := FileAccess.open(CHOICE, FileAccess.WRITE)
	if f:
		f.store_string(k)

## Library path the extension config names, read without loading it.
func _lib(cfg: String) -> String:
	var c := ConfigFile.new()
	if c.load(cfg) != OK:
		return ""
	var p: String = c.get_value("libraries", "linux.release.x86_64", "")
	return ProjectSettings.globalize_path(p)
