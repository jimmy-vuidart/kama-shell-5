# 16 — Le canal du shell n'a pas de version

- **Priorité :** faible
- **Nature :** dette
- **Zone :** `crates/ipc/src/lib.rs`, `crates/compositor/src/ipc.rs`, `crates/ui/src/compositor.rs`
- **Se vérifie :** par tests unitaires
- **Certitude :** lu dans le code

## Constat

Les messages du canal (`crates/ipc`) ne portent aucun numéro de version, et
rien n'est échangé à la connexion.

Ce qui tient déjà :

- une commande inconnue est journalisée et ignorée par le compositeur ;
- un événement inconnu est ignoré par le dock ;
- un champ en trop est ignoré des deux côtés ;
- les champs ajoutés récemment (`simulated`, `screen`) ont une valeur par
  défaut.

Ce qui ne tient pas : les champs sans valeur par défaut (`screens`, `windows`,
`habits`, et tous ceux de `Screen`, `Window`, `Habit`). En ajouter un casse
la lecture par un programme plus ancien, et en retirer un, celle par un plus
récent.

## Conséquence

`scripts/install.sh` remplace les binaires pendant que la session tourne. Le
compositeur garde l'ancien code jusqu'à la prochaine connexion ; le dock, lui,
est relancé par `coquille-dock-watch` s'il tombe, et « Paramètres » est
relancé à chaque ouverture. Après une mise à jour, des programmes de deux
versions se parlent donc couramment. Un état illisible laisse le dock vide,
sans message clair.

## Piste

1. Un premier événement `hello` avec un numéro de protocole, envoyé à la
   connexion. Un programme qui lit un numéro plus grand que le sien le dit
   dans son journal (et le dock peut l'afficher).
2. Une règle écrite en tête de la crate : tout champ ajouté a une valeur par
   défaut ; un champ ne se retire qu'en montant le numéro.
3. Garder dans les tests un exemplaire de l'état tel que chaque version
   l'écrivait, lu par la version courante. Un tel test existe déjà pour une
   version (`a_state_from_an_older_compositor_has_real_screens`).

## Terminé quand

- Le compositeur annonce sa version à la connexion ; un dock plus ancien
  l'ignore sans erreur (test).
- Les états des versions passées sont lus par le code courant (tests).

## Liens

- Aucun.
