# Phase 5 — Session autonome

À terme, coquille sera **le seul bureau installé** sur le poste. Le
compositeur et le dock sont déjà indépendants. La session, elle, repose
encore sur des services que fournissent Plasma ou GNOME, ou que personne ne
fournit. La phase 5 les reconstruit, dans l'ordre ci-dessous, pour qu'on
puisse retirer Plasma et GNOME sans rien perdre d'essentiel.

Bilan fait le 18 octobre 2026, sur le code et les paquets installés, pas
sur une session coquille en marche (la session du moment était Plasma).

## Déjà indépendant

Compositeur, XWayland et gestionnaire de fenêtres X11, décorations, mode
jeu ; dock (barre, lanceur, zone de notification : la crate `system-tray`
tient son propre `StatusNotifierWatcher`, sans `kded6` ; le dock tient
aussi la zone de notification X11 des icônes Wine et Proton, sans
`xembedsniproxy`) ; centre de
contrôle et mixeur (`pactl`, `nmcli`, `bluetoothctl`, `ddcutil`, outils
indépendants de tout bureau) ; écran « Paramètres » ; fond d'écran ;
captures d'écran depuis le tiroir ; déconnexion, redémarrage, extinction
(`systemctl`) ; thème d'icônes Reversal.

## Dépendances directes vers un autre bureau

| Dépendance | Paquet | Gravité |
|---|---|---|
| `gsettings get org.gnome.desktop.interface icon-theme` (`apps/src/icons.rs`) | glib2, gsettings-desktop-schemas | faible : ni l'un ni l'autre n'est un bureau, et ce n'est qu'un repli sans Reversal |
| écran de connexion `plasmalogin` | **plasma-login-manager** | haute : c'est aussi lui qui déverrouille le trousseau (PAM) |

## Étapes

Ordre conseillé : le plus d'effet pour le moins de code d'abord, le plus
gros morceau (la capture pour le partage d'écran) ensuite.

- [ ] 1. **Notifications.** Aujourd'hui, `org.freedesktop.Notifications` est
      activé par D-Bus via `plasma_waitforname`, qui attend `plasmashell` :
      sous coquille, les notifications de Discord, Steam ou Firefox se
      perdent déjà. Un serveur dans le dock (ou un programme à part) qui
      prend le nom, affiche des bulles en layer-shell, garde un historique
      dans le centre de contrôle. C'est le moment de concevoir le concept
      conservé de la phase 4 : jamais sur l'écran d'un jeu ancré, « Ne pas
      déranger » automatique pendant le jeu, bascule du dock enfin active.
- [ ] 2. **Agent polkit.** Sans lui, toute demande de mot de passe
      administrateur échoue en silence (mises à jour graphiques, montage d'un
      disque interne, GParted…). Aujourd'hui `polkit-kde-agent`, lancé
      seulement par Plasma. Un petit programme iced (`coquille-polkit`, dans
      le style du shell) ; `polkit-gnome` peut servir en attendant.
- [ ] 3. **Portails, cible systemd et démarrage automatique.**
      - `coquille-portals.conf` (installé par `scripts/install.sh`) : aucun
        `.portal` ne déclare `UseIn=coquille`, le choix dépend donc des replis
        de `xdg-desktop-portal`. Désigner gtk par défaut (sélecteur de
        fichiers…), gnome-keyring pour les secrets, puis wlr pour la capture
        une fois l'étape 4 faite.
      - Une cible `coquille-session.target` (`BindsTo=graphical-session.target`,
        `Wants=xdg-desktop-autostart.target`), démarrée après
        `dbus-update-activation-environment` et arrêtée avec le compositeur :
        les entrées de `~/.config/autostart` et `/etc/xdg/autostart` (Discord,
        Steam, arch-update-tray…) et les services liés à la session graphique
        démarrent enfin. `OnlyShowIn=` / `NotShowIn=` des entrées
        d'autostart : vérifier que les composants KDE et GNOME ne partent pas
        (ils filtrent sur `XDG_CURRENT_DESKTOP`).
- [ ] 4. **Veille, extinction des écrans, verrouillage.** Rien n'existe :
      les écrans ne s'éteignent jamais, rien ne se verrouille. Détail et
      questions ouvertes : tickets 22 (veille) et 23 (verrouillage).
      - `ext-idle-notify-v1` et `idle-inhibit-unstable-v1` (une vidéo ou un
        jeu ancré empêche la mise en veille) ;
      - extinction des écrans (DPMS) après un délai réglable, rallumage au
        premier mouvement ;
      - `ext-session-lock-v1` : hyprlock ou swaylock d'abord, un écran de
        verrouillage à nous plus tard ;
      - bouton « Mettre en veille » dans le dock (`systemctl suspend`), et
        verrouillage avant la veille (inhibiteur `logind` « delay »).
- [ ] 5. **Capture d'écran pour le partage** (Discord, navigateur, OBS). Le
      backend GNOME des portails exige Mutter, celui de KDE exige KWin.
      Implémenter `ext-image-copy-capture-v1` et `ext-image-capture-source-v1`
      (voire `wlr-screencopy`, plus répandu) pour `xdg-desktop-portal-wlr`,
      avec sa propre sélection d'écran ou un sélecteur à nous. Le plus gros
      morceau de la phase. Attention aux écrans ancrés : la capture d'un jeu
      ne doit pas casser l'affichage direct ni le VRR.
- [x] 6. **Clair / sombre et thèmes des applis.** Fait le 18 octobre 2026
      (voir « Thème des applis » dans `CLAUDE.md`) : réglages GNOME
      (`color-scheme`, `gtk-theme`, `icon-theme` s'il a deux variantes),
      `settings.ini` de GTK, portail gtk pour les réglages, Qt par sa
      platform theme `gtk3`. Vérifié en imbriqué sur un bus isolé : Dolphin,
      Calculatrice (libadwaita), pavucontrol (GTK 4), une fenêtre GTK 3,
      au démarrage et à chaud. Restes :
      - sans `~/.config/kdeglobals` (installation qui n'a jamais connu
        Plasma), les applis KDE prennent un style par défaut illisible ;
        avec un `kdeglobals` qui nomme son style (Breeze), tout va bien ;
      - applis Qt 5 (VLC 3) : leur platform theme `gtk3` existe aussi, mais
        rien n'a été essayé avec elles ;
      - Plasma ne réécrit peut-être pas ces réglages en démarrant : après
        une session coquille en clair, une session Plasma en sombre peut
        garder des applis GTK 3 claires jusqu'au premier changement de thème
        (à vérifier).
- [ ] 7. **Touches multimédia.** Les raccourcis ne gèrent que `Mod+…` et les
      terminaux virtuels. Volume, muet (`pactl`, déjà utilisé par le dock),
      lecture / pause / suivant (MPRIS), `Impr. écran` (capture existante,
      celle du tiroir). À appliquer aussi pendant un jeu : ce sont des
      touches matérielles, comme `Boost`.
- [ ] 8. **Écran de connexion.**
      - Remplacer plasma-login-manager (SDDM seul, ou greetd avec un
        greeter) en reportant `pam_gnome_keyring` dans sa configuration PAM.
        Ne rien changer sans l'accord de Jimmy (voir « Sécurité pendant le
        développement » dans `CLAUDE.md`).
      - [x] Pont des icônes X11 : fait, le dock est la zone de notification
        XEmbed (`dock/src/backend/xembed.rs`), `xembedsniproxy` n'est plus
        lancé.

## Après les étapes

- [ ] 9. Trousseau : n'en garder qu'un. gnome-keyring et kwallet sont
      installés tous les deux ; gnome-keyring est léger et n'a pas besoin de
      GNOME.
- [ ] 10. Agent Bluetooth dans le dock : jumeler un appareil qui demande une
      confirmation passe aujourd'hui par `bluedevil`.
- [ ] 11. Éclairage nocturne : `wlr-gamma-control-unstable-v1`, puis la
      bascule du dock (gammastep d'abord, ou à nous).
- [ ] 12. Presse-papiers persistant : `wl-clip-persist` fonctionne déjà
      (data-control est implémenté), il suffit de le lancer avec la session.
- [ ] 13. Agent de secrets NetworkManager (mot de passe Wi-Fi) : seulement si
      le poste passe un jour au Wi-Fi.

## Protocoles Wayland secondaires

Pas indispensables, mais des applis les attendent : `wp-tearing-control`
(jeux, en complément du VRR), `presentation-time` (ticket 24),
`xdg-foreign` (dialogues des portails rattachés à leur fenêtre),
`keyboard-shortcuts-inhibit` (machines virtuelles, bureau à distance),
`foreign-toplevel`, sélection primaire (limite connue),
`single-pixel-buffer`.

## Paquets

- **À garder** (pas des bureaux) : pipewire et wireplumber, networkmanager,
  bluez, udisks2, upower, power-profiles-daemon, xdg-user-dirs,
  xdg-desktop-portal et xdg-desktop-portal-gtk, gnome-keyring, glib2 et
  dconf, wl-clipboard, kitty.
- **À remplacer avant de retirer Plasma et GNOME** : plasma-login-manager,
  polkit-kde-agent, le démon de
  notifications de plasmashell. Et les applis du quotidien venues de KDE ou
  GNOME (Dolphin ou Nautilus, Spectacle, Discover…) : elles peuvent rester
  seules, mais tirent une part de leur bureau en dépendance.

## Fin de phase

Une session coquille ouverte depuis l'écran de connexion, Plasma et GNOME
désinstallés (ou leurs services masqués pour l'essai), où : une
notification s'affiche, une demande polkit aboutit, Discord partage un
écran, les écrans s'éteignent puis se verrouillent, Discord et Steam
démarrent seuls, une appli libadwaita suit le thème sombre.
