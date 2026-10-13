# 03 — Un écran éteint puis rallumé reste noir

- **Priorité :** haute
- **Nature :** robustesse
- **Zone :** `crates/compositor/src/backend/udev.rs`
- **Se vérifie :** sur le matériel seulement
- **Certitude :** absence d'écoute lue dans le code ; écran noir à confirmer

## Constat

Les écrans sont cherchés une fois, au démarrage (`scan`, `udev.rs:359`). Le
backend n'écoute que les vblanks et les erreurs DRM (`udev.rs:296`) : aucune
source udev ne signale qu'un connecteur a changé.

La limite « pas de branchement à chaud » est connue et notée dans
`CLAUDE.md`. Ce ticket porte sur sa conséquence la plus courante.

## Conséquence

Un écran DisplayPort qu'on éteint par son bouton se débranche du point de vue
du pilote, puis se rebranche quand on le rallume. Sans nouveau réglage de mode
de la part du compositeur, il reste probablement noir jusqu'à la fin de la
session. Les deux écrans en DisplayPort (DP-1 et DP-2, dont le centre) sont
concernés.

Pendant ce temps, chaque image de cet écran échoue : le journal reçoit
« cannot draw a frame » toutes les 240 tentatives, et un minuteur est réarmé
à chaque période.

## Piste

Portée volontairement réduite : **retrouver un écran déjà connu**, pas en
accueillir un nouveau.

1. Ajouter la source `UdevBackend` de Smithay et réagir à ses événements
   `Changed`.
2. Garder le `DrmScanner` dans `Udev` et relancer `scan_connectors`.
3. Connecteur connu qui disparaît : l'écran est marqué éteint, on ne tente
   plus de le dessiner. Ses fenêtres restent où elles sont dans le modèle.
4. Connecteur connu qui revient : même CRTC, même mode, même rôle ; on refait
   le réglage de mode, puis un redessin complet.

Hors portée : un écran inconnu, un changement de rôle ou de disposition. Ils
restent décidés au démarrage.

## Terminé quand

- Essai avec Jimmy, depuis la session matérielle, pour chacun des écrans en
  DisplayPort : éteindre l'écran, attendre dix secondes, le rallumer ; il
  retrouve son image et ses fenêtres.
- Le journal dit quel écran est parti et lequel est revenu, sans avalanche de
  « cannot draw a frame » entre les deux.
- `--probe` fonctionne toujours.

## Liens

- Voisin de : 22 (éteindre les écrans par DPMS passe par le même code).
