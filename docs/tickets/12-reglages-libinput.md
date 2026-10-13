# 12 — Aucun périphérique d'entrée n'est réglé

- **Priorité :** faible
- **Nature :** évolution
- **Zone :** `crates/compositor/src/backend/udev.rs`, `config.rs`, `crates/config`, `crates/settings`
- **Se vérifie :** sur le matériel seulement
- **Certitude :** lu dans le code

## Constat

`process_input_event` (`input.rs:118`) ne traite que le clavier, les
mouvements, les boutons et la molette. `InputEvent::DeviceAdded` tombe dans
le cas par défaut : aucun périphérique libinput n'est configuré. La souris
garde donc les réglages par défaut de libinput, dont le profil d'accélération
adaptatif.

## Conséquence

Pas de moyen de choisir une accélération plate, une vitesse du pointeur ou le
sens de la molette. Les jeux qui lisent le mouvement brut (`relative-pointer`,
valeurs non accélérées) ne sont pas touchés ; le pointeur du bureau, si.

## À décider avec Jimmy

- Quels réglages exposer : profil d'accélération (plat ou adaptatif), vitesse,
  sens de la molette ? Autre chose ?
- Dans `config.toml` seulement, ou aussi dans l'écran « Paramètres » ?

## Piste

1. Une section `[mouse]` dans `config.toml`, lue strictement comme les autres
   (`deny_unknown_fields`), avec son test d'aller-retour si « Paramètres »
   l'écrit.
2. À `DeviceAdded`, appliquer les réglages aux périphériques qui ont la
   capacité pointeur.
3. Les réappliquer à `reload_config`, sur les périphériques gardés en liste.

## Terminé quand

- Essai avec Jimmy sur le matériel : le profil choisi dans le fichier
  s'applique à la souris au démarrage, et après un rechargement à chaud.
- Le modèle `config/config.toml` documente la section.

## Liens

- Partage la liste des périphériques avec : 11.
