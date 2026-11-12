# 36 — Une image de latence de trop : dessiner au plus tard avant le vblank

- **Priorité :** moyenne
- **Nature :** ressenti (latence de frappe, de défilement, de glisser)
- **Zone :** `crates/compositor/src/backend/udev.rs`
- **Se vérifie :** sur le matériel seulement
- **Certitude :** mécanisme lu dans le code ; écart avec GNOME à mesurer

## Constat

Au vblank, `frame_done` envoie les « frame callbacks » puis, si un changement
attendait, dessine l'écran **aussitôt**. L'appli, prévenue au même instant,
dessine son image suivante et la valide quelques millisecondes plus tard :
l'écran a déjà été dessiné sans elle. Son image attend le vblank suivant pour
être composée, puis encore un pour être montrée : **deux rafraîchissements**
entre le dessin de l'appli et l'écran.

GNOME (mutter, « dynamic max render time ») dessine au contraire **le plus
tard possible** avant le vblank, d'après le temps que ses images ont pris :
ce que l'appli a validé pendant la période y entre, **un rafraîchissement**
seulement. À 60 Hz, c'est 16 ms de moins sur chaque frappe, chaque cran de
molette et chaque mouvement de fenêtre. Le pointeur, lui, n'est plus
concerné depuis qu'il est sur le plan curseur.

## Piste

1. Mesurer le temps de chaque image (début de `render_screen` jusqu'au
   `queue_frame`, et la fin du rendu GPU par la barrière que rend Smithay),
   par écran, et l'écrire au journal en `debug`.
2. Au vblank, ne pas dessiner tout de suite : armer un minuteur à
   `vblank + période − (temps d'image prévu + marge)`, le temps prévu étant un
   maximum glissant des dernières images (comme mutter). Les « frame
   callbacks » partent toujours au vblank.
3. Une image ratée (montrée un vblank trop tard) agrandit la marge ; jamais
   sur l'écran d'un jeu en VRR ni en affichage direct, où l'image du jeu part
   dès qu'elle arrive.
4. Réglage de secours (`COQUILLE_RENDER_EARLY=1`) pour revenir au dessin
   immédiat.

Le ticket 20 (cadence en logique pure) rend ces règles testables.

## Terminé quand

- Avec un outil de mesure (caméra rapide, ou `wayland-latency`), la frappe
  dans kitty gagne environ un rafraîchissement.
- Aucune image ratée de plus au journal, flou du verre actif sur les trois
  écrans.

## Liens

- Dépend de : 20. Suit : 01.
