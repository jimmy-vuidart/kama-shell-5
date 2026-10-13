# 19 — Une appli installée en cours de session n'a ni nom ni icône

- **Priorité :** faible
- **Nature :** bug
- **Zone :** `crates/compositor/src/state.rs`, `shell.rs`, `crates/dock/src/app.rs`, `crates/apps`
- **Se vérifie :** en imbriqué
- **Certitude :** lu dans le code

## Constat

Le catalogue des applis (les fichiers `.desktop`) est lu une seule fois :

- par le compositeur, à sa création (`Catalog::load()`, `state.rs:289`) ;
- par le dock, à son démarrage (`dock/src/app.rs:234`).

Le cache d'icônes du compositeur retient aussi les échecs
(`decoration/icons.rs`, « a name that led nowhere is kept too ») : une icône
introuvable une fois le reste.

## Conséquence

Une appli installée pendant la session :

- n'apparaît pas dans le lanceur avant le redémarrage du dock ;
- s'ouvre avec un nom tiré de son identifiant et un carré de couleur, sur sa
  barre de titre et sa capsule, jusqu'à la fin de la session.

## Piste

Sans nouvelle dépendance :

- **Compositeur :** quand `identity` ne trouve rien pour un `app_id`, relire
  le catalogue, au plus une fois toutes les quelques secondes, puis chercher
  à nouveau. Vider alors les échecs du cache d'icônes.
- **Dock :** relire le catalogue à l'ouverture du lanceur, hors du fil de
  l'interface (`spawn_blocking`, comme pour les icônes), et remplacer la liste
  si elle a changé.

Surveiller les dossiers `applications` (inotify) serait plus direct, mais
demande une crate de plus.

## Terminé quand

- Essai en imbriqué : un fichier `.desktop` ajouté dans un dossier
  `XDG_DATA_HOME` d'essai pendant que tout tourne. À l'ouverture suivante du
  lanceur, l'appli y est ; sa fenêtre s'ouvre avec son nom et son icône.
- La relecture ne se fait pas dans le chemin de rendu du compositeur (voir le
  ticket 08).

## Liens

- Voisin de : 08.
