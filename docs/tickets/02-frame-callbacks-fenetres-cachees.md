# 02 — Les fenêtres cachées ne reçoivent pas de « frame callback »

- **Priorité :** haute
- **Nature :** bug
- **Zone :** `crates/compositor/src/backend/winit.rs` (`send_frame_callbacks`, commun aux deux backends)
- **Se vérifie :** en imbriqué
- **Certitude :** mécanisme lu dans le code ; gel du client à confirmer

## Constat

`send_frame_callbacks` (`winit.rs:284`) ne sert que les fenêtres dont
`geometry.visible` est vrai et qui sont sur l'écran qui vient d'afficher une
image. N'en reçoivent donc jamais :

- un onglet qui n'est pas celui affiché dans son emplacement ;
- une fenêtre réduite ;
- une fenêtre sous un jeu ancré ;
- un panneau sur un écran ancré.

## Conséquence

Un client qui attend ce signal pour rendre son image suivante reste bloqué
tant qu'il est caché. C'est le cas d'`eglSwapBuffers` avec la synchronisation
verticale (Mesa) et de Vulkan en mode FIFO. Or mettre des applis en onglets
est l'usage normal du projet (« pages web ou Discord, en onglets »).

Symptômes possibles : une appli figée quand on revient sur son onglet, un jeu
ou un lecteur vidéo dont le fil principal s'arrête, du son qui se coupe.

## Piste

Un minuteur de secours, vers 1 Hz, qui envoie le signal aux surfaces qui ne
sont visibles sur aucun écran. `Window::send_frame` prend déjà un délai
minimal (`throttle`) : le passer à une seconde suffit à ne jamais dépasser ce
rythme. niri fait de même (« fallback timer »).

Les fenêtres visibles gardent leur chemin actuel.

## Terminé quand

- Essai scripté en imbriqué : un client qui rend avec la synchronisation
  verticale (`glxgears`, ou un client SDL) est mis en onglet caché ; avec
  `WAYLAND_DEBUG=1`, il reçoit encore `wl_callback.done` environ une fois par
  seconde.
- Même essai avec une fenêtre réduite.
- Une fenêtre visible ne reçoit pas plus de signaux qu'avant.

## Liens

- Aucun.
