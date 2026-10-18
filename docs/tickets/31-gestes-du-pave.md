# 31 — Gestes du pavé tactile

- **Priorité :** faible
- **Nature :** évolution
- **Zone :** `crates/compositor/src/input.rs`, `backend/udev.rs`
- **Se vérifie :** sur le portable
- **Certitude :** absence lue dans le code

## Constat

Le pavé tactile clique, glisse et défile (ticket 30), mais les gestes à
trois ou quatre doigts que libinput rapporte (`GestureSwipe*`,
`GesturePinch*`) tombent dans le cas par défaut de `process_input_event`.
Le protocole `pointer-gestures` n'est pas offert : les applis (Firefox, les
visionneuses) ne reçoivent ni pincement ni balayage.

## Piste

1. Offrir `zwp_pointer_gestures_v1` (Smithay le fournit) et relayer les
   gestes à la surface sous le pointeur.
2. À décider avec Jimmy : un geste du shell (trois doigts vers le haut pour
   le tiroir ?). La règle d'or (« sans Super, la souris ne fait que ce que
   fait l'appli ») plaide pour ne rien prendre aux applis.

## Terminé quand

Pincer zoome dans Firefox ; le geste du shell, s'il y en a un, est décidé.
