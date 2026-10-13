# 20 — Sortir la cadence des images en logique pure

- **Priorité :** architecture (à faire avant le ticket 01)
- **Nature :** dette, tests
- **Zone :** `crates/compositor/src/backend/udev.rs`, `game.rs`
- **Se vérifie :** par tests unitaires
- **Certitude :** lu dans le code

## Constat

`udev.rs` fait 798 lignes et n'a aucun test. Ses chemins d'affichage direct
et de VRR n'ont jamais été exécutés. C'est le code le plus risqué du projet,
et le seul que ni les tests ni le mode imbriqué ne couvrent.

Une partie de ce fichier n'a pourtant rien de matériel : la machine qui
décide quand un écran peut être redessiné. Elle tient dans l'énumération
`Pacing` (`udev.rs:67`) et dans trois méthodes de `State` qui se partagent
ses transitions :

- `redraw_screens` (`udev.rs:530`) : qui est dû, qui attend ;
- `render_screen` (`udev.rs:556`) : image envoyée, vide ou en échec, minuteur
  ou non ;
- `frame_done` (`udev.rs:748`) : retour au repos, redessin demandé
  entre-temps.

Le projet sait déjà faire autrement : les décisions du mode jeu sont dans
`game.rs` (`Boost`, `StickyEdges`, `ModTap`), pures et testées.

## Conséquence

Le défaut du ticket 01 est invisible sans le matériel, et sa correction le
serait aussi. Toute retouche de la cadence se fait à l'aveugle.

## Piste

Un type pur, à côté de `Boost`, qui reçoit des faits et rend des décisions :

- faits : « quelque chose a changé sur cet écran », « l'image est partie »,
  « l'image était vide », « l'image a échoué », « le vblank est arrivé », « le
  minuteur est tombé », « la session est en pause, revient » ;
- décisions : « dessiner maintenant », « armer un minuteur de telle durée »,
  « envoyer les frame callbacks », « ne rien faire ».

`udev.rs` ne garde que l'application de ces décisions à DRM et à calloop.

Ce ticket ne change aucun comportement : il déplace, et pose les tests de ce
qui existe. Le ticket 01 change ensuite les règles.

## Terminé quand

- Les transitions actuelles sont couvertes par des tests unitaires, dont les
  cas : image vide, échec répété, redessin demandé pendant l'attente, retour
  de VT.
- `udev.rs` ne contient plus de décision de cadence, seulement leur
  application.
- `--probe` fonctionne toujours, et la CI locale est verte.

## Liens

- Prépare : 01, 24.
