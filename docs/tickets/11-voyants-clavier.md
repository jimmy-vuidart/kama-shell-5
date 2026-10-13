# 11 — Les voyants du clavier ne s'allument pas

- **Priorité :** faible
- **Nature :** bug
- **Zone :** `crates/compositor/src/handlers/mod.rs`, `backend/udev.rs`
- **Se vérifie :** sur le matériel seulement
- **Certitude :** lu dans le code

## Constat

`SeatHandler` (`handlers/mod.rs:39`) ne redéfinit pas `led_state_changed`,
qui ne fait rien par défaut dans Smithay. Le backend matériel ne garde pas
non plus la liste des claviers : `process_input_event` ignore
`InputEvent::DeviceAdded`.

## Conséquence

Sur le matériel, Verr. Maj et Verr. Num changent bien d'état, mais leurs
voyants ne suivent pas.

## Piste

1. Dans `udev.rs`, garder les périphériques libinput qui ont la capacité
   clavier (`DeviceAdded`, `DeviceRemoved`).
2. Implémenter `led_state_changed` : pour chaque clavier, `led_update` avec
   l'état reçu. Rien à faire en imbriqué.
3. Donner l'état courant à un clavier qui vient d'être branché.

## Terminé quand

- Essai avec Jimmy sur le matériel : les voyants suivent Verr. Maj et
  Verr. Num, y compris sur un clavier branché en cours de session.

## Liens

- Partage la liste des périphériques avec : 12.
