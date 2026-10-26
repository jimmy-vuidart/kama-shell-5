# 33 — Pas de partage d'écran

- **Priorité :** haute
- **Nature :** fonction manquante
- **Zone :** `crates/compositor` (protocoles, `render/`, `backend/udev.rs`), `config/coquille-portals.conf`, `crates/dock` (sélecteur, voyant)
- **Se vérifie :** protocole et portail en imbriqué, écrans ancrés sur le matériel
- **Certitude :** absence lue dans le code ; la prise en charge d'`ext-image-copy-capture` par `xdg-desktop-portal-wlr` est à vérifier sur la version installée

## Constat

Le compositeur ne laisse aucun programme lire ce qu'il affiche. La capture
d'écran du tiroir (`screenshot.rs`) se fait à l'intérieur du compositeur et
écrit un fichier ; aucun protocole de capture n'est offert (ni
`ext-image-copy-capture-v1`, ni `wlr-screencopy`).

Côté portail, `coquille-portals.conf` ne nomme que `Secret` et `Settings`.
Pour `ScreenCast` et `Screenshot`, `xdg-desktop-portal` prend ce qu'il
trouve : le backend GNOME exige Mutter, celui de KDE exige KWin. La demande
échoue.

## Conséquence

Discord ne partage ni un écran ni une fenêtre, en raid comme en appel. Pareil
pour le partage d'écran des navigateurs (Meet, Teams) et pour OBS. Pour un
usage quotidien, c'est le manque le plus visible de la session.

## Deux chemins

**A. Le protocole standard et `xdg-desktop-portal-wlr`.**
Le Smithay copié dans `vendor/smithay` fournit `ext-image-copy-capture-v1`
et `ext-image-capture-source-v1` (`src/wayland/image_copy_capture`,
`src/wayland/image_capture_source`). Le compositeur répond aux demandes de
copie, et `xdg-desktop-portal-wlr` fait le flux PipeWire. Pour capturer une
fenêtre, il faut aussi `ext-foreign-toplevel-list-v1` (fourni lui aussi :
`src/wayland/foreign_toplevel_list`) et sa source de capture. Le choix de
l'écran ou de la fenêtre passe par la commande `chooser_cmd` de xdpw : un
outil existant d'abord (`slurp`, un menu), un sélecteur du dock ensuite.

**B. Comme niri : l'interface de Mutter.**
Le compositeur sert lui-même `org.gnome.Mutter.ScreenCast` et
`org.gnome.Mutter.DisplayConfig` sur D-Bus et produit le flux PipeWire (crate
`pipewire`), et `xdg-desktop-portal-gnome` fait le reste, sélecteur compris.
Plus de code dans le compositeur, mais zéro copie (dmabuf) maîtrisé de bout
en bout et le curseur en métadonnée.

Recommandation : **A d'abord**, moins de code et rien qui imite un autre
bureau ; B seulement si A s'avère trop lent sur la NVIDIA (3440×1440 à
pleine cadence).

## À décider avec Jimmy

- Le sélecteur : un outil existant suffit-il, ou un sélecteur du dock dans
  le style du shell dès le départ ?
- Un voyant de partage en cours (dans la barre du dock), et un moyen de
  l'arrêter depuis le dock ?
- Pendant un partage, cacher les bulles de notification (comme en jeu) ?

## Piste

1. Offrir `ext-image-capture-source-v1` (sources : écran, puis fenêtre) et
   `ext-image-copy-capture-v1`. Rendre dans le tampon du client par le même
   chemin que `screenshot.rs` (scène composée, pixels physiques de l'écran,
   curseur à part ou dessiné selon la demande) ; tampons shm et dmabuf ;
   répondre avec les dommages pour ne pas tout recopier.
2. Portail : `org.freedesktop.impl.portal.ScreenCast=wlr` et
   `org.freedesktop.impl.portal.Screenshot=wlr` dans `coquille-portals.conf`,
   et la configuration de xdpw (`chooser_type`, `chooser_cmd`). Lancer le
   portail avec la session (ticket 35).
3. **Écrans ancrés** : capturer le jeu ne doit casser ni l'affichage direct
   ni le VRR. Copier le tampon du jeu tel quel quand il est seul à l'écran
   plutôt que forcer la composition, et ne pas faire remonter la fréquence
   (une capture n'est pas une image affichée).
4. Une fenêtre capturée mais cachée (onglet non affiché, réduite, derrière
   un jeu) : la rendre hors écran, et lui envoyer ses « frame callbacks »
   (ticket 02), sinon son flux se fige.

## Terminé quand

- En imbriqué : un flux `pipewiresrc` (GStreamer) ou la page de test WebRTC
  d'un navigateur montre un écran simulé, puis une fenêtre, à la bonne
  échelle et avec le curseur.
- Sur le matériel, avec Jimmy : Discord partage l'écran du centre pendant un
  jeu ancré, sans saccade, et le journal dit toujours
  `direct scanout: the buffer of the game is shown as it is` (ou explique
  pourquoi non) ; Discord partage aussi une fenêtre d'un écran latéral.
- Aucun partage ne démarre sans un choix fait dans le sélecteur.

## Liens

- Dépend de : 35 (portails lancés avec la session).
- Partage du code avec : 02 (fenêtres cachées).
- Étape 5 de la phase 5 (`docs/phase-5.md`).
