# 37 — Redimensionner : la décoration court devant l'appli

- **Priorité :** moyenne
- **Nature :** ressenti (finition)
- **Zone :** `crates/compositor/src/input.rs` (`resize_floating`), `shell.rs`, `render/scene.rs`
- **Se vérifie :** en imbriqué
- **Certitude :** lu dans le code ; effet à capturer

## Constat

Tirer un bord d'une fenêtre flottante, ou la séparation de deux colonnes,
change aussitôt le rectangle du modèle : barre de titre, anneau, ombre et
rognage passent à la nouvelle taille au mouvement suivant. L'appli, elle, ne
dessine à cette taille qu'après avoir reçu le configure : pendant une ou deux
images, son contenu est plus petit que son cadre (fond vide visible) ou rogné.
À chaque mouvement, le cadre « tremble » devant le contenu.

GNOME et Windows déplacent cadre et contenu **ensemble** : le cadre suit la
taille que l'appli a effectivement dessinée.

L'appli sait au moins qu'on la redimensionne : l'état xdg `resizing` est
annoncé pendant le geste (GTK s'en sert pour dessiner plus simplement).

## Piste

1. Pendant un `Gesture::Resizing`, dessiner le cadre à la taille **validée**
   par l'appli (sa géométrie xdg courante), ancré du côté opposé au bord tiré,
   et non au rectangle du modèle ; le modèle garde la taille demandée.
2. Ne pas envoyer de nouveau configure tant que le précédent n'est pas
   acquitté (`ack_configure`), en gardant seulement le dernier rectangle
   voulu : l'appli n'est pas noyée sous les tailles intermédiaires.
3. Même chose pour les colonnes (`Splitting`), fenêtre par fenêtre.
4. Les applis X11 n'ont pas d'acquittement : garder le chemin actuel.

## Terminé quand

- Capture ralentie en imbriqué d'un redimensionnement de `gnome-text-editor`
  et de kitty : aucun fond vide entre le cadre et le contenu.
- Le redimensionnement reste sans animation (règle des gestes tenus).

## Liens

- Aucun.
