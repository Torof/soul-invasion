class_name Scale
extends RefCounted
## The world's scale (D40): everything from GROWN_FROM to TRUE_UP_TO
## metres is drawn at its true height (a body 1.7 m, a reed 2 m); above, the
## height eases towards reality without reaching it, so a 20 m oak stays
## tall and does not fill the screen: TRUE_UP_TO × (h / TRUE_UP_TO) ^ ABOVE.
## The small things, which the grass swallowed, are drawn bigger: SMALL_TIMES
## their height up to SMALL_UP_TO (a mushroom of 8 cm shows 16), then
## growing more slowly up to GROWN_FROM, so a taller thing is never drawn
## shorter than a smaller one.

const TRUE_UP_TO := 3.0
const ABOVE := 0.5
const SMALL_UP_TO := 0.3
const SMALL_TIMES := 2.0
const GROWN_FROM := 1.0


## The height something `metres` tall is drawn at, in metres.
static func shown(metres: float) -> float:
	if metres <= SMALL_UP_TO:
		return maxf(metres, 0.0) * SMALL_TIMES
	var small := SMALL_UP_TO * SMALL_TIMES
	if metres < GROWN_FROM:
		return small + (metres - SMALL_UP_TO) * (GROWN_FROM - small) / (GROWN_FROM - SMALL_UP_TO)
	if metres <= TRUE_UP_TO:
		return metres
	return TRUE_UP_TO * pow(metres / TRUE_UP_TO, ABOVE)
