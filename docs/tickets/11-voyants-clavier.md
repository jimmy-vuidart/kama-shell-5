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

## État

- Implémenté :
  - `SeatHandler::led_state_changed` (`handlers/mod.rs`) transmet le nouvel état des voyants au backend.
  - `Backend::update_led_state` (`backend/mod.rs`) et `Udev::update_led_state` (`backend/udev.rs`) mettent à jour les voyants de tous les claviers via `libinput_device_led_update`.
  - `Udev` conserve la liste des périphériques clavier (`keyboard_devices`) sur `DeviceAdded` et `DeviceRemoved`.
  - Lors de la connexion d'un clavier, l'état courant des voyants lui est transmis immédiatement.
- Reste à vérifier sur le matériel physique avec Jimmy.

## Terminé quand

- Essai avec Jimmy sur le matériel : les voyants suivent Verr. Maj et
  Verr. Num, y compris sur un clavier branché en cours de session.

## Liens

- Partage la liste des périphériques avec : 12.
