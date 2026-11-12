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

## The recentre button's tooltip.
const RECENTRE := "Revenir au corps"

## Development only (F12 with a --dev server): a test line for the speech
## bubble, in turn, until people talk.
const DEV_SPEECH := [
	["Voix d'essai", "Ceci est une ligne d'essai pour la bulle de dialogue."],
	["Voix d'essai", "Et voici la suivante : elle remplace la première."],
]

## PLACEHOLDER, development only (--dev): what body 3 says when the
## piloted body comes near it, until NPCs really talk (DevGreeter).
const DEV_GREETINGS := [
	"Ah, salut ! Ça fait longtemps.",
	"Ah, t'es là !",
	"Viens m'aider.",
	"Viens voir par ici !",
	"Te voilà enfin !",
	"Hé, par ici !",
	"Tu tombes bien, toi.",
]


## The words for a reason the way is barred; an unknown one reads as no
## way seen.
static func blocked(reason: String) -> String:
	return str(BLOCKED.get(reason, BLOCKED["no_way"]))


## A body's name as a speaker, until bodies have names.
static func body_name(body: int) -> String:
	return "Corps %d" % body
