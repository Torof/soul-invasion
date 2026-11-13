class_name UiWords
extends RefCounted
## Every French line of the interface, kept in one place: the body's
## thoughts, the buttons, the development lines. Nothing else here.

## What the piloted body thinks when its way is barred, for each reason
## the server gives (`body_blocked`'s and `move_refused`'s `reason`).
const BLOCKED := {
	"water": "Bloqué : l'eau est trop profonde",
	"rock": "Bloqué : la falaise barre le chemin",
	"no_way": "Bloqué : aucun passage en vue",
}

## What the piloted body thinks as it decides how to go on (`body_thinks`,
## protocol v22), by the thought's kind (and side, for skirting).
const THOUGHTS := {
	"skirting_left": "Pas de passage ici… je longe vers la gauche",
	"skirting_right": "Pas de passage ici… je longe vers la droite",
	"way_seen": "Je vois un passage, j'y vais",
	"no_way": "Je ne sais pas où aller",
}

## The recentre button's tooltip.
const RECENTRE := "Revenir au corps"

## Development only (F12 with a --dev server): a test line for the speech
## bubble, in turn, until people talk.
const DEV_SPEECH := [
	["Voix d'essai", "Ceci est une ligne d'essai pour la bulle de dialogue."],
	["Voix d'essai", "Et voici la suivante : elle se range sous la première."],
	["Autre voix", "Une troisième, d'une autre voix, pour voir le fil."],
]


## The words for a reason the way is barred; an unknown one reads as no
## way seen.
static func blocked(reason: String) -> String:
	return str(BLOCKED.get(reason, BLOCKED["no_way"]))


## The words for a thought; an unknown one reads as not knowing where to
## go.
static func thought(kind: String, side: String) -> String:
	var key := "%s_%s" % [kind, side] if kind == "skirting" else kind
	return str(THOUGHTS.get(key, THOUGHTS["no_way"]))
