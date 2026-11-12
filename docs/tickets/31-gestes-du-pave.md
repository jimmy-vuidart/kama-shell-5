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
Le protocole `pointer-gestures` n'était pas offert : les applis (Firefox, les
visionneuses) ne recevaient ni pincement ni balayage.

## Piste

1. *Fait, jamais essayé sur le portable* : `zwp_pointer_gestures_v1` est
   offert, et `State::touchpad_gesture` relaie balayages, pincements et
   maintiens à la surface sous le pointeur (rien pendant un geste du shell).
   Le maintien sert aussi à GTK pour arrêter un défilement inertiel.
2. À décider avec Jimmy : un geste du shell (trois doigts vers le haut pour
   le tiroir ?). La règle d'or (« sans Super, la souris ne fait que ce que
   fait l'appli ») plaide pour ne rien prendre aux applis.

## Terminé quand

Pincer zoome dans Firefox ; le geste du shell, s'il y en a un, est décidé.
