# 32 — Un écran externe branché au portable

- **Priorité :** faible
- **Nature :** évolution
- **Zone :** `backend/udev.rs`, `outputs.rs`, `input.rs` (capot)
- **Se vérifie :** sur le portable
- **Certitude :** absence lue dans le code

## Constat

Les écrans ne sont trouvés qu'au démarrage (ticket 03). Un écran externe
branché au portable en cours de session n'apparaît pas ; fermer le capot
avec un écran externe laisse l'écran intégré allumé (logind ne met pas en
veille quand la machine est « dockée », `HandleLidSwitchDocked=ignore`).

## Piste

1. Le branchement à chaud du ticket 03 d'abord.
2. Capot fermé et écran externe présent : éteindre l'`eDP` (DPMS) et le
   retirer du bureau comme un écran débranché ; le rallumer à l'ouverture.
3. Les rôles : l'écran externe le plus large devient-il le centre, et
   l'`eDP` un côté ? À décider avec Jimmy.

## Terminé quand

Brancher un écran l'ajoute, fermer le capot éteint l'écran intégré sans
mettre la machine en veille, et le rouvrir le rend.

## Liens

- Dépend de : 03.
