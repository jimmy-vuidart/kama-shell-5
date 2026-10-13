# 04 — XWayland n'est jamais relancé

- **Priorité :** haute
- **Nature :** robustesse
- **Zone :** `crates/compositor/src/xwayland.rs`
- **Se vérifie :** en imbriqué
- **Certitude :** lu dans le code

## Constat

Quand le serveur X s'arrête, `XwmHandler::disconnected` (`xwayland.rs:449`)
journalise, efface `xdisplay` et oublie les fenêtres X11. Rien ne le relance.
De même si le serveur échoue pendant son démarrage (`XWaylandEvent::Error`,
`xwayland.rs:88`).

## Conséquence

Les jeux Proton passent par XWayland. Si le serveur X tombe une fois, plus
aucun jeu ne démarre avant d'avoir fermé puis rouvert la session, donc perdu
toutes les fenêtres ouvertes.

## Piste

1. Dans `disconnected`, relancer `start_xwayland` après un court délai
   (minuteur calloop).
2. Borner les relances : au-delà de quelques chutes rapprochées, abandonner
   et le dire dans le journal, comme le fait `coquille-dock-watch`.
3. Points à traiter en chemin :
   - `xwm_state` fait un `expect` sur `self.xwm` (`xwayland.rs:239`) :
     l'ancien gestionnaire doit rester en place jusqu'à ce que le nouveau le
     remplace ;
   - le numéro d'affichage peut changer. Les programmes déjà lancés (le dock
     en premier) gardent l'ancien `DISPLAY` dans leur environnement. Soit on
     redemande le même numéro, soit on prévient le dock ; avec
     `--export-environment`, il faut aussi le redire aux services de la
     session.

## Terminé quand

- Essai scripté en imbriqué : une appli X11 ouverte, `pkill Xwayland`, puis
  une appli X11 lancée à nouveau s'ouvre.
- Une appli X11 lancée depuis le dock après la relance s'ouvre aussi.
- Un serveur X qui tombe en boucle n'est pas relancé sans fin.

## Liens

- Aucun.
