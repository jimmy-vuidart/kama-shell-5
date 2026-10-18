# 24 — Protocole `wp_presentation`

- **Priorité :** évolution
- **Nature :** fonction manquante
- **Zone :** `crates/compositor/src/state.rs`, `backend/udev.rs`, `backend/winit.rs`
- **Se vérifie :** protocole en imbriqué, horodatages sur le matériel
- **Certitude :** absence lue dans le code

## Constat

Le compositeur n'offre pas `wp_presentation` (« presentation-time »). Par ce
protocole, un client apprend à quel instant précis chacune de ses images est
arrivée à l'écran, et quelle est la période de rafraîchissement.

Aujourd'hui, un client ne reçoit que le « frame callback », daté du moment où
le compositeur l'envoie.

## Conséquence

- Les lecteurs vidéo (mpv) s'en servent pour caler l'image sur le son et
  éviter les images doublées ou sautées : sans lui, ils devinent.
- Des jeux et des couches de traduction s'en servent pour leur cadence, et
  les outils de mesure pour leurs chiffres.

Ce n'est pas un défaut visible à coup sûr, mais il touche les deux usages
principaux du projet : la vidéo sur un écran latéral et le jeu au centre.

## Piste

1. Créer le global avec `PresentationState` de Smithay.
2. À chaque image, recueillir les retours demandés par les surfaces dessinées
   sur l'écran.
3. Au vblank (`DrmEvent::VBlank`), les signaler avec l'horodatage et le
   numéro de séquence que donne le pilote ; les surfaces qui n'ont pas été
   montrées reçoivent « discarded ».
4. En imbriqué : l'heure courante et la période annoncée, sans prétendre à
   mieux.

La cadence sortie en logique pure (ticket 20) et les horodatages de vblank du
ticket 01 sont les deux pièces dont ce ticket a besoin.

## Terminé quand

- En imbriqué, `WAYLAND_DEBUG=1 mpv` montre des événements
  `wp_presentation_feedback.presented` pour ses images.
- Sur le matériel, avec Jimmy : mpv n'annonce plus d'images perdues sur une
  vidéo à la fréquence de l'écran.

## Liens

- Dépend de : 20, 01.
- Cité parmi les protocoles secondaires de la phase 5.
