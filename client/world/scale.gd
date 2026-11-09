class_name Scale
extends RefCounted
## The world's scale (D40): everything under TRUE_UP_TO metres is drawn at
## its true height (a body 1.7 m, a reed 2 m, a mushroom 8 cm); above, the
## height eases towards reality without reaching it, so a 20 m oak stays
## tall and does not fill the screen: TRUE_UP_TO × (h / TRUE_UP_TO) ^ ABOVE.

const TRUE_UP_TO := 3.0
const ABOVE := 0.5


## The height something `metres` tall is drawn at, in metres.
static func shown(metres: float) -> float:
	if metres <= TRUE_UP_TO:
		return maxf(metres, 0.0)
	return TRUE_UP_TO * pow(metres / TRUE_UP_TO, ABOVE)
