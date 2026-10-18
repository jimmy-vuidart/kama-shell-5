# 22 — Mise en veille des écrans

- **Priorité :** évolution
- **Nature :** fonction manquante
- **Zone :** `crates/compositor` (protocoles, `backend/udev.rs`), `crates/config`, `crates/settings`
- **Se vérifie :** protocoles en imbriqué, extinction sur le matériel
- **Certitude :** absence lue dans le code

## Constat

Le compositeur n'a aucune notion d'inactivité : les trois écrans restent
allumés tant que la session dure. Il n'offre pas non plus les protocoles qui
vont avec :

- `ext-idle-notify-v1`, par lequel un programme apprend que l'utilisateur est
  absent ;
- `idle-inhibit`, par lequel un lecteur vidéo ou un jeu demande de ne pas
  mettre en veille.

## Conséquence

Des écrans allumés toute la nuit si la session reste ouverte : usure et
consommation.

## À décider avec Jimmy

- Le délai avant l'extinction, et s'il se règle dans « Paramètres ».
- Pendant un jeu ancré : jamais de veille, ou seulement si le jeu le demande ?
- Les trois écrans ensemble, ou chacun selon ce qui s'y passe (une vidéo à
  gauche garde-t-elle le centre allumé) ?
- Ce qui réveille : toute entrée, ou le clavier et les boutons seulement (une
  souris qui bouge quand on pose quelque chose sur le bureau) ?

## Piste

1. Offrir `idle-inhibit` et `ext-idle-notify-v1` (Smithay les fournit).
2. Un minuteur d'inactivité, remis à zéro par chaque entrée.
3. Extinction et rallumage par DPMS dans le backend matériel. Le rallumage
   passe par le même code que le retour d'un écran (ticket 03).
4. Un réglage dans `config.toml`, relu à chaud.

## Terminé quand

- Sur le matériel, avec Jimmy : les écrans s'éteignent après le délai, se
  rallument à la première entrée, et retrouvent leur image.
- Une vidéo en lecture empêche la veille.
- Le réglage à zéro désactive tout.

## Liens

- Partage du code avec : 03.
- Va naturellement avec : 23.
- Étape 4 de la phase 5 (`docs/phase-5.md`).
