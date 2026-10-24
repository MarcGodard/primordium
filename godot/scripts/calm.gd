## Calms the day/night strobe at the small scales: when a sim day passes in a second or two,
## light and tide are drawn as a running average over real time, so the scene breathes instead
## of flashing. The model still runs the full cycle; only what is drawn is smoothed.
extends RefCounted

var value := -1.0
var last_t := -1.0
var rate := 0.0

## `target` this frame, at sim day `t`, after `delta` real seconds. Returns what to draw.
func step(target: float, t: float, delta: float) -> float:
	if value < 0.0 or delta <= 0.0:
		value = target
		last_t = t
		return value
	# sim days per real second, itself smoothed so a hitch does not snap the light
	var now: float = max(t - last_t, 0.0) / delta
	last_t = t
	rate += (now - rate) * min(1.0, delta * 2.0)
	# a day in under ~2 s: average over about half a day of real time; slower: follow closely
	var tau: float = clamp(0.5 / max(rate, 0.01), 0.05, 2.0) if rate > 0.5 else 0.05
	value += (target - value) * (1.0 - exp(-delta / tau))
	return value
