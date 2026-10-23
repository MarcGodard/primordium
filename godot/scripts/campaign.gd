## The game layer: one goal per scale, each read off that scale's own sim (`goal_progress()` on
## the view). Reaching it opens the next world in the hub and writes a codex page. Creatures kept
## at the reef go to the player's own world, the garden. Saved to user://campaign.json.
extends Node

signal changed
signal toast(title: String, body: String, color: Color)
## A goal was reached: main shows the graduation card that leads to the next world.
signal graduated(k: int, page: String)

var save_path := "user://campaign.json"
## Index = view index (0 elements .. 6 reef, 7 garden). Title, what to do, codex page on success.
const GOALS := [
	["Meet the atoms of life", "Inspect the backbone atom, a side-group letter and a noble gas (arrows or click), then try another universe's table.",
		"[b]The atoms of life.[/b] One shell rule makes the whole table. The backbone atom has room for a chain plus a side group, the letters hang off it and carry information, the noble gases bond with nothing. A table with a different shell size makes different chemistry, and maybe none."],
	["Build a molecule", "Let sunlit atoms bond until one molecule reaches %d atoms.",
		"[b]Molecules.[/b] Energy from the sun lets atoms make and break bonds. Most fall apart, but some shapes last longer, and the lasting ones pile up. That is the first selection, before anything is alive."],
	["A chain that copies itself", "Watch chains copy each other until one lineage is %d copies deep.",
		"[b]Replication.[/b] A chain whose letters pair with partners can serve as a template: dry, wet, dry again, and a copy peels off. Copies have mistakes, so lineages of chains drift and the better copiers win."],
	["A bubble that divides", "Wait for a protocell to grow and split, %d generations deep.",
		"[b]Protocells.[/b] Oily molecules close into bubbles. A bubble that traps a good copier grows faster, stretches and splits: now the chain's success is the bubble's success. Inside versus outside is the start of an individual."],
	["Living cells", "Let cells divide and evolve for %d generations.",
		"[b]Cells.[/b] Genomes now write genes through a genetic code. Promoters switch genes on with light, cold or crowding. Mutation, predators and the tide sort genomes: this is evolution by natural selection, running on its own."],
	["Travellers", "Let %d cells ride the ocean current from one beach to another.",
		"[b]Biogeography.[/b] Each beach has its own climate by latitude. Lineages that drift on the current to a new beach either fit or die, so beaches end up with their own kinds, like islands with their own birds."],
	["Find a creature", "Find a body of %d+ cells with %d+ different cell jobs that crawls (gold rings, G jumps to one) and keep it with K.",
		"[b]Multicellular life.[/b] Cells of one body share a genome but switch it differently by where they sit: some hold on, some feed, some shade. A body that divides labour and moves is an animal in all but name. On the open reef they appear now and then and are gone again."],
	["A creature that breeds true", "In your world, one lineage makes creatures on %d of the last %d days. Keep the best again to breed them.",
		"[b]Breeding true.[/b] A lucky body is not yet a kind of animal. When one lineage keeps producing creatures, generation after generation, the trait is inherited: selection, yours and the reef's, has made a species."],
]
const NEXT := ["Chemistry", "Polymers", "Protocells", "Cells", "Coast", "Reef", "Your world"]
const KEPT_MAX := 60
const NAMES := ["Ara", "Bel", "Cor", "Dax", "Eon", "Fen", "Gil", "Hux", "Ivo", "Jun", "Kel", "Lum"]

var state := {"unlocked": 0, "done": {}, "codex": [], "kept": [], "garden": [], "run": 1, "explore": false, "named": 0}
## Last progress per view index, for the HUD banner and hub cards.
var progress := {}
var _timer := 0.0
## When this session began: time away is measured to here, not to a later garden visit.
var session_start := Time.get_unix_time_from_system()
## Off in captures: they must not touch the player's save.
var persist := true

func _ready() -> void:
	for a in OS.get_cmdline_user_args():
		if a.begins_with("--shot="):
			persist = false
	# --save=PATH: a capture with its own save file, to test saving and loading
	for a in OS.get_cmdline_user_args():
		if a.begins_with("--save="):
			save_path = a.trim_prefix("--save=")
			persist = true
	if persist:
		load_state()

func load_state() -> void:
	if not FileAccess.file_exists(save_path):
		return
	var f := FileAccess.open(save_path, FileAccess.READ)
	if f == null:
		return
	var d = JSON.parse_string(f.get_as_text())
	if d is Dictionary:
		for k in d:
			state[k] = d[k]
		# saves from before the name counter: continue past the names already handed out
		if not d.has("named"):
			state.named = state.kept.size() + state.codex.size()

func save() -> void:
	if not persist:
		return
	var f := FileAccess.open(save_path, FileAccess.WRITE)
	if f:
		f.store_string(JSON.stringify(state))

## World k open in the hub? The garden opens with the first kept creature.
func is_open(k: int) -> bool:
	if state.explore:
		return true
	if k == 7:
		return not state.kept.is_empty()
	return k <= int(state.unlocked)

func is_done(k: int) -> bool:
	return state.done.has(str(k))

func goal_title(k: int) -> String:
	return GOALS[k][0] if k < GOALS.size() else "Your world"

func set_explore(on: bool) -> void:
	if on and not finished():
		toast.emit("Explore mode opens at the end", "Reach every goal first, from the atom to a creature that breeds true.", Color(0.8, 0.85, 0.95))
		return
	state.explore = on
	save()
	changed.emit()

## Poll the active view once a second; a newly met goal opens the next world.
func check(k: int, view: Node) -> void:
	if k >= GOALS.size() or not view.has_method("goal_progress"):
		return
	var p: Dictionary = view.goal_progress()
	if p.is_empty():
		return
	progress[k] = p
	if p.get("done", false) and not is_done(k):
		complete(k, p.get("who", ""))

func complete(k: int, who: String) -> void:
	state.done[str(k)] = Time.get_datetime_string_from_system()
	state.unlocked = max(int(state.unlocked), k + 1)
	var page: String = GOALS[k][2]
	if who != "":
		page += " First reached by %s." % who
	state.codex.append({"title": GOALS[k][0], "text": page, "scale": k})
	save()
	graduated.emit(k, page)
	changed.emit()

## Every goal reached: explore mode (all worlds, any order) is the reward.
func finished() -> bool:
	return is_done(GOALS.size() - 1)

## Nothing reached and nothing kept yet: a new player, who starts at the smallest scale.
func is_new() -> bool:
	return state.done.is_empty() and state.kept.is_empty() and not state.explore

func tick(delta: float, k: int, view: Node) -> void:
	_timer -= delta
	if _timer <= 0.0:
		_timer = 1.0
		check(k, view)

## Goal text with its numbers filled in.
func goal_text(k: int) -> String:
	var p: Dictionary = progress.get(k, {})
	var s: String = GOALS[k][1]
	var args: Array = p.get("args", [])
	# numbers come from the view's first poll; until then show the sentence without them
	return s % args if args.size() == s.count("%d") else s.replace("%d", "N")

## Keep a creature (from a reef run or your world); it goes into the garden on the next visit.
## Returns its name, or "" when the collection is full of creatures not yet released.
func keep(c: Dictionary, where: String) -> String:
	if state.kept.size() >= KEPT_MAX:
		var i := -1
		for j in state.kept.size():
			if state.kept[j].get("released", false):
				i = j
				break
		if i < 0:
			toast.emit("Collection full", "Visit your world (8) to release the creatures you kept, then keep more.", Color(0.95, 0.6, 0.5))
			return ""
		state.kept.remove_at(i)
	state.named = int(state.named) + 1
	var n: int = state.named
	var name := "%s-%d" % [NAMES[n % NAMES.size()], n]
	var rec := {"name": name, "genome": Array(c.genome), "mass": Array(c.mass), "cells": c.cells, "jobs": c.jobs, "creature": c.creature, "from": where, "released": false}
	state.kept.append(rec)
	if c.creature:
		state.codex.append({"title": "Creature " + name, "text": "[b]%s[/b], kept from %s: %d cells doing %d jobs, crawling." % [name, where, c.cells, c.jobs], "scale": 6})
	save()
	changed.emit()
	if c.creature and not is_done(6):
		complete(6, "%s from %s" % [name, where])
	return name

## Fast-forward reports kept as codex pages, newest last; only the latest few.
const FIELD_NOTES := 12
func field_note(title: String, text: String) -> void:
	state.codex.append({"title": title, "text": text, "scale": -2})
	var notes := []
	for i in state.codex.size():
		if int(state.codex[i].get("scale", 0)) == -2:
			notes.append(i)
	if notes.size() > FIELD_NOTES:
		state.codex.remove_at(notes[0])
	save()
	changed.emit()

func new_run() -> int:
	state.run = int(state.run) + 1
	save()
	return int(state.run)
