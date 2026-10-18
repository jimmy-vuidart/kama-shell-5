# 28 — Dock : messages non lus réels, glisser une icône vers un écran

- **Priorité :** faible (étape optionnelle de la phase 4)
- **Nature :** évolution
- **Zone :** `crates/dock`
- **Se vérifie :** en imbriqué
- **Certitude :** absence lue dans le code

## Constat

- Le badge des messages non lus existe, mais seule la simulation (`--mock`)
  en montre.
- Glisser une icône du tiroir vers un écran pour l'y ouvrir n'est pas fait ;
  le menu « Ouvrir sur » de la barre le remplace.

## Piste

1. **Non-lus** : écouter `com.canonical.Unity.LauncherEntry` sur D-Bus
   (Discord, Thunderbird, Telegram l'émettent), rattacher le compte à l'appli
   par son fichier `.desktop`.
2. **Glisser** : la commande « ouvrir sur un écran » existe déjà dans le
   canal du shell ; il manque le geste, qui quitte la surface du tiroir.
   À concevoir avec Jimmy (aperçu de l'écran visé ? emplacement précis ?).

## Terminé quand

- Un message reçu dans Discord fait apparaître le badge, qui disparaît une
  fois lu.
- Une icône lâchée sur un écran y ouvre l'appli, sans changer sa mémoire.

## Liens

- Aucun.
