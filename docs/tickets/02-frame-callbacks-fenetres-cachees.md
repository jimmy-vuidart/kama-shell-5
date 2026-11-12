# 02 — Les fenêtres cachées ne recevaient pas de « frame callback »

- **Priorité :** haute
- **Nature :** bug
- **Zone :** `crates/compositor/src/backend/winit.rs` (`send_hidden_frame_callbacks`, commun aux deux backends), `main.rs`
- **Se vérifie :** en imbriqué
- **État :** **fait dans le code, pas encore essayé**

## Constat

`send_frame_callbacks` ne sert que les fenêtres visibles sur l'écran qui
vient d'afficher une image. Un onglet de derrière, une fenêtre réduite ou une
fenêtre sous un jeu ne recevaient jamais le signal : un client qui l'attend
pour dessiner (`eglSwapBuffers` avec synchronisation verticale, Vulkan FIFO)
restait figé, et montrait une image périmée un instant quand on revenait sur
son onglet.

## Ce qui a été fait

Un minuteur d'une seconde (`HIDDEN_FRAME_PERIOD`, armé dans `main.rs`)
appelle `send_hidden_frame_callbacks` : chaque fenêtre dont la géométrie dit
`visible = false` reçoit le signal, au plus une fois par période (le
« throttle » de Smithay, avec une sortie principale nulle : c'est le chemin
prévu pour une surface vue nulle part). Les fenêtres visibles gardent leur
chemin.

## Terminé quand

- Essai scripté en imbriqué : `glxgears` (ou un client SDL) mis en onglet
  caché reçoit encore `wl_callback.done` environ une fois par seconde
  (`WAYLAND_DEBUG=1`).
- Même essai avec une fenêtre réduite.
- Une fenêtre visible ne reçoit pas plus de signaux qu'avant.

## Liens

- Aucun.
