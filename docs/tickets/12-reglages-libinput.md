# 12 — Aucun périphérique d'entrée n'est réglé

- **Priorité :** faible
- **Nature :** évolution (faite, à vérifier)
- **Zone :** `crates/compositor/src/backend/udev.rs`, `config.rs`, `crates/config`, `crates/settings`
- **Se vérifie :** sur le matériel seulement
- **Certitude :** implémenté, jamais essayé sur le matériel

## Constat (avant la correction)

`process_input_event` (`input.rs:118`) ne traite que le clavier, les
mouvements, les boutons et la molette. `InputEvent::DeviceAdded` tombe dans
le cas par défaut : aucun périphérique libinput n'est configuré. La souris
garde donc les réglages par défaut de libinput, dont le profil d'accélération
adaptatif.

## Conséquence

Pas de moyen de choisir une accélération plate, une vitesse du pointeur ou le
sens de la molette. Les jeux qui lisent le mouvement brut (`relative-pointer`,
valeurs non accélérées) ne sont pas touchés ; le pointeur du bureau, si.

## État

- Implémenté :
  - Section `[mouse]` dans `config.toml` (`accel_speed`, `accel_profile`, `natural_scroll`, `left_handed`).
  - Prise en charge dans `coquille-config` (`Settings`, `Change`, `apply`).
  - Application libinput dans `crates/compositor/src/backend/udev.rs` à l'ajout de périphérique (`DeviceAdded`) et au rechargement de configuration (`reload_config`).
  - Page « Souris » ajoutée à l'application `coquille-settings` avec curseur de vitesse (-100..=100), choix de profil (Adaptatif / Plat), défilement naturel et inversion des boutons pour gauchers.
- Reste à vérifier sur le matériel physique avec Jimmy.

## Terminé quand

- Essai avec Jimmy sur le matériel : le profil choisi dans le fichier ou l'app s'applique à la souris au démarrage, et après un rechargement à chaud.
- Le modèle `config/config.toml` documente la section (fait).

## Liens

- Partage la liste des périphériques avec : 11.
