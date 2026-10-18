# 30 — Essayer coquille sur le portable

- **Priorité :** haute
- **Nature :** vérification
- **Zone :** `backend/udev.rs`, `scale.rs`, `xwayland.rs`, dock (`backend/backlight.rs`, `backend/battery.rs`)
- **Se vérifie :** sur le portable, avec Jimmy
- **Certitude :** écrit et vérifié en imbriqué, jamais exécuté sur le portable

## Constat

La mise à l'échelle, l'écran unique, le pavé tactile, le rétroéclairage, la
batterie et la veille sont faits (voir « État du portable » dans
`CLAUDE.md`). Le backend matériel ne s'essaie que sur la vraie machine.

## À essayer, dans l'ordre

1. `coquille --probe` : GPU utilisé, connecteur `eDP`, mode 4K, échelle lue
   dans `[scale]`. **Si le portable est hybride** (iGPU qui tient l'écran,
   NVIDIA à part), un jeu rendu sur la NVIDIA passe par PRIME : vérifier
   qu'il s'affiche, et à quelle vitesse, avant le reste.
2. Session à 200 % : décorations nettes, curseur à la bonne taille, dock et
   « Paramètres » nets ; passer à 150 % depuis « Paramètres » sans redémarrer.
3. Applis X11 (Steam, un jeu Proton) : nettes, Steam à la bonne taille
   (`STEAM_FORCE_DESKTOPUI_SCALING`), un jeu plein écran à la définition de
   l'écran et `direct scanout: the buffer of the game is shown as it is`
   dans `session.log`. `xwayland.log` ne doit pas se plaindre de `xrdb`.
4. Pavé tactile : toucher pour cliquer, clic à deux doigts, défilement
   naturel et inertiel, pavé coupé pendant la frappe ; chaque réglage changé
   dans « Paramètres » s'applique aussitôt.
5. Luminosité : le curseur du centre de contrôle, les touches (aussi en jeu).
   Si logind refuse, `dock.log` dit `logind refused the brightness`.
6. Batterie : pourcentage et temps restant, alerte à 10 % (débrancher).
7. Capot fermé puis rouvert : la machine dort, puis l'écran retrouve son
   image (`lid` dans `session.log`) ; « Mettre en veille » fait de même.
8. Jeu sur l'écran unique : `Super` ouvre le tiroir par-dessus, Discord
   lancé depuis le tiroir vient par-dessus, un clic dans le jeu le remet
   devant.

## Terminé quand

Tout ce qui précède marche, ou a son propre ticket.

## Liens

- Lié à : 03 (écran externe et branchement à chaud), 22 et 23 (veille,
  verrouillage).
