# 35 — Rien ne démarre avec la session

- **Priorité :** haute
- **Nature :** fonction manquante
- **Zone :** `scripts/coquille-session`, `crates/compositor/src/main.rs` (`export_environment`), nouveau `config/coquille-session.target`, `config/coquille-portals.conf`, `scripts/install.sh`
- **Se vérifie :** dans la vraie session, ouverte depuis l'écran de connexion
- **Certitude :** lu dans le code ; la liste des entrées de démarrage automatique est à relever sur le poste

## Constat

- `coquille-session` lance le trousseau, puis remplace son processus par le
  compositeur (`exec`), qui lance le dock. Rien d'autre.
- Avec `--export-environment`, le compositeur donne son environnement aux
  services de l'utilisateur (`dbus-update-activation-environment --systemd`),
  mais n'atteint aucune cible systemd : `graphical-session.target` et
  `xdg-desktop-autostart.target` ne démarrent jamais.
- `coquille-portals.conf` ne nomme que `Secret` et `Settings` : les autres
  portails (sélecteur de fichiers, ouvrir avec…) dépendent des replis de
  `xdg-desktop-portal` pour un bureau qu'il ne connaît pas.

## Conséquence

- Discord, Steam et tout ce qui est dans `~/.config/autostart` ou
  `/etc/xdg/autostart` ne démarrent pas avec la session.
- Les services de l'utilisateur liés à la session graphique
  (`WantedBy=graphical-session.target`) ne démarrent pas non plus, et ceux
  qui tournent ne s'arrêtent pas quand on quitte.
- Le sélecteur de fichiers d'une appli Flatpak ou d'un navigateur qui passe
  par le portail peut ne pas s'ouvrir.

## À décider avec Jimmy

- **Qui porte la session** : garder le compositeur lancé par
  `coquille-session`, qui démarre et arrête une cible ; ou, comme niri, faire
  du compositeur un service de l'utilisateur (`coquille.service`,
  `Type=notify`) que `coquille-session` démarre et attend
  (`systemctl --user --wait start`). Le second arrête tout proprement même
  après un plantage, mais déplace les journaux vers `journalctl` (ou les
  garde dans `session.log` en le disant au service).
- Quelles entrées de démarrage automatique garder : relever ensemble
  `/etc/xdg/autostart` et `~/.config/autostart`.

## Piste

1. Une cible `coquille-session.target`, installée dans
   `~/.config/systemd/user/` :
   `BindsTo=graphical-session.target`, `Before=graphical-session.target`,
   `Wants=graphical-session-pre.target xdg-desktop-autostart.target`,
   `After=graphical-session-pre.target`.
2. Le compositeur la démarre juste après `export_environment` (seulement
   quand il possède les services de la session, c'est-à-dire avec
   `--export-environment`), et l'arrête en quittant. Un plantage la laisse
   active : d'où la question du service ci-dessus.
3. Le générateur de systemd (`systemd-xdg-autostart-generator`) lit
   `OnlyShowIn=` et `NotShowIn=` d'après `XDG_CURRENT_DESKTOP=coquille` :
   les composants réservés à KDE ou à GNOME restent à l'écart. Vérifier les
   entrées sans filtre une à une (`xembedsniproxy`, `polkit-kde-agent`,
   `gnome-keyring-*`, `arch-update-tray`…), et masquer celles qui gêneraient
   par `Hidden=true` dans `~/.config/autostart`, jamais en les supprimant.
4. Portails : `default=gtk` dans `coquille-portals.conf`, en gardant les
   lignes `Secret` et `Settings` ; `ScreenCast` et `Screenshot` viendront
   avec le ticket 33.
5. `wl-clip-persist` (presse-papiers qui survit à l'appli qui l'a copié)
   peut devenir un service lié à la cible.

## Terminé quand

- Session ouverte depuis l'écran de connexion, avec Jimmy :
  `systemctl --user is-active graphical-session.target` répond `active` ;
  Discord et Steam démarrent seuls ; aucun composant de Plasma ni de GNOME
  ne tourne (`ps`) en dehors de ceux choisis.
- Le sélecteur de fichiers de Firefox s'ouvre par le portail
  (`widget.use-xdg-desktop-portal.file-picker=1`).
- Après « Se déconnecter », la cible est `inactive` et les applis lancées
  par elle sont arrêtées.
- Une session lancée à la main depuis un terminal virtuel (sans
  `--export-environment`) ne touche à aucune cible.

## Liens

- Prépare : 33 (portail de capture), 34 (agent polkit).
- Étape 3 de la phase 5 (`docs/phase-5.md`).
