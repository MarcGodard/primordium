## Words for the table being played, for guide text: backbone, letters, the heritable pair and
## the codon length all come from the shell rule, so one guide reads right on every table.
extends RefCounted

## Keyed by the table: static vars survive the script reload of an in-game table switch.
static var _cache := {}
static var _key := PackedByteArray()

static func words() -> Dictionary:
	var key: PackedByteArray = ElementsSim.letters_z()
	if not _cache.is_empty() and key == _key:
		return _cache
	_key = key
	var sim := ElementsSim.new()
	sim.load(PackedByteArray([]))
	var els: Array = sim.elements()
	var s: Dictionary = sim.summary()
	var name := func(z: int) -> String: return els[z - 1].name
	var letters: PackedByteArray = s.letters
	var names := PackedStringArray()
	for z in letters:
		names.append(name.call(z))
	var her: PackedByteArray = s.heritable
	var bonds: int = els[s.backbone - 1].valence
	var codon := 1
	while (1 << codon) < letters.size() + 1:
		codon += 1
	var nums := ["", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven", "twelve", "thirteen", "fourteen", "fifteen"]
	_cache = {
		"backbone": name.call(s.backbone),
		"bonds": nums[bonds],
		"side": "one side group" if bonds == 3 else "one side group and %s Hy cap%s" % [nums[bonds - 3], "" if bonds == 4 else "s"],
		"letters": ", ".join(names.slice(0, names.size() - 1)) + " or " + names[names.size() - 1],
		"n_letters": nums[letters.size()] if letters.size() < nums.size() else str(letters.size()),
		"gene_a": name.call(her[0]),
		"gene_b": name.call(her[1]),
		"codon": nums[codon],
	}
	sim.free()
	return _cache
