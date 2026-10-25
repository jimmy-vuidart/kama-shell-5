# coquille-dock

Le dock de coquille : une barre fine sur le bord d'un écran, le lanceur
d'applications qu'elle ouvre, ses notifications, son centre de contrôle et
son mixeur par application. Un seul programme, plusieurs surfaces layer-shell, un seul état.

Chaque écran peut avoir sa barre, dans un sens ou dans l'autre :

- **verticale** : colonne sur le bord gauche ou sur le bord droit, qui se
  déploie au survol ; le lanceur est un tiroir sur toute la hauteur, du même
  côté. À droite tout est en miroir : les icônes restent contre le bord, les
  noms se déploient vers la gauche, le menu et le mixeur s'ouvrent à gauche ;
- **horizontale** : rangée centrée sur le bord bas, qui nomme l'icône
  survolée dans une étiquette ; le lanceur est un tiroir sur toute la
  largeur, qui monte du bord bas, en quatre colonnes (applications, jeux,
  notifications, centre de contrôle) ; seules les applications s'élargissent
  avec l'écran, et sur un écran très large les colonnes restent au milieu.

Dans les deux lanceurs, les notifications et le centre de contrôle ont le
même aspect : dans le tiroir vertical, ils se partagent son pied
(notifications à gauche, centre de contrôle à droite, quel que soit le côté
de la barre) ; dans le lanceur horizontal, ce sont ses deux dernières
colonnes.

Il tourne **dans coquille** : il lui faut le protocole layer-shell et, pour
montrer les vraies fenêtres, le socket du compositeur (`COQUILLE_SOCKET`,
transmis aux programmes que coquille lance).

## Lancer

Tout simulé, dans coquille imbriqué (rien n'est lu ni modifié sur le système,
hors la liste des applications installées et la ludothèque Steam) :

```bash
cargo build -p coquille -p coquille-dock
```

```bash
target/debug/coquille --role left --spawn "$PWD/target/debug/coquille-dock --mock"
```

Avec les vraies fenêtres et les vrais services (son, réseau, Bluetooth,
luminosité), toujours en imbriqué :

```bash
target/debug/coquille --three-screens --spawn kitty --spawn "$PWD/target/debug/coquille-dock"
```

En imbriqué `--three-screens`, l'écran gauche simulé est trop étroit pour le
tiroir (640 px) : `--role left` donne un écran gauche de la taille de la
fenêtre.

## Dans la session

`scripts/install.sh` installe `coquille-dock` à côté de `coquille`, et
`coquille-session` le lance avec le compositeur par `coquille-dock-watch`,
qui le relance s'il tombe (cinq chutes d'affilée et il renonce).

- Journal : `~/.local/state/coquille/dock.log`, celui de la session d'avant
  dans `dock.previous.log`.
- `COQUILLE_DOCK_ARGS` passe des options au dock ; `COQUILLE_NO_DOCK=1`
  démarre la session sans lui.
- Le dock dessine avec le GPU. S'il ne s'affiche pas, `ICED_BACKEND=tiny-skia`
  le fait dessiner sans.

## Options

| Option | Effet |
|---|---|
| `--mock` | Simule tout : fenêtres, son, réseau, Bluetooth, luminosité, batterie, session. |
| `--backends a,b,…` | Les services réels à utiliser, les autres étant simulés : `compositor`, `audio`, `network`, `bluetooth`, `brightness`, `session`, `tray`, `notifications`, `battery`. Sans l'option : tous. |
| `--output NOM` | L'écran où s'afficher. Par défaut, celui que le compositeur désigne comme écran gauche. |
| `--config FICHIER` | Fichier de configuration (défaut : `~/.config/coquille/config.toml`). |
| `--state-dir DOSSIER` | Où garder les volumes par application (défaut : `~/.local/state/coquille`, et un dossier temporaire avec `--mock`). |
| `--drawer` | Ouvre le tiroir dès le démarrage (pour une capture). |

`RUST_LOG=coquille_dock=debug` détaille ce que fait le dock.

**Session.** Se déconnecter, Mettre en veille, Redémarrer et Éteindre
demandent toujours une confirmation, puis agissent pour de vrai : la
déconnexion demande au compositeur de quitter, la veille, le redémarrage et
l'extinction passent par `systemctl suspend`, `systemctl reboot` et
`systemctl poweroff`. Si le système refuse, sa raison
s'affiche sous le nom de l'utilisateur. Seul `--mock` (ou une liste
`--backends` sans `session`) les simule : **en imbriqué sans `--mock`,
Redémarrer redémarre vraiment la machine.**

## Configuration

Dans `~/.config/coquille/config.toml`, partagé avec le compositeur (modèle :
`config/config.toml`) :

```toml
theme = "dark"
accent = "#7aa7ff"

[dock]
favorites = ["firefox", "discord", "kitty", "code", "obsidian"]

[dock.screens]
left = "vertical"      # "vertical", "vertical-right", "horizontal" ou "off"
center = "off"
right = "horizontal"

[drawer]
pinned = ["firefox", "discord", "kitty", "code", "obsidian", "mpv", "org.gnome.Nautilus"]
width = 640            # tiroir d'un dock vertical
height = 600           # lanceur d'un dock horizontal
```

Sans `[dock.screens]`, il y a un dock vertical sur l'écran gauche et aucun
ailleurs. **Un écran seul (un portable) a toujours le sien** : quand le
fichier ne dit rien des écrans branchés et qu'aucun n'aurait de dock, le
premier (le centre d'abord) reçoit un dock vertical ; dès que le fichier
nomme un écran branché, il est pris au mot (`off` compris). La règle est
dans `coquille-config` (`docks_for`), partagée avec l'écran « Paramètres ».
Toutes les barres montrent les mêmes applis ; « Ici » y désigne l'écran de
la barre. Avec un seul écran, le menu n'offre pas « Ouvrir sur » et les
étiquettes ne nomment pas d'écran. La touche `Super` ouvre le lanceur sur
l'écran du pointeur s'il a un dock, sinon sur le premier écran qui en a un.

**En jeu sur un seul écran**, aucun écran n'est libre : `Super` ouvre le
lanceur par-dessus le jeu (le compositeur y montre alors les panneaux de cet
écran). Une appli déjà ouverte qu'on y lance vient par-dessus le jeu au lieu
de démarrer une seconde fois ; un clic dans le jeu le remet devant.

Un nom est celui d'un fichier `.desktop` (sans l'extension), sa dernière
partie (`nautilus` pour `org.gnome.Nautilus`), le programme qu'il lance, ou
ce que les fenêtres annoncent (`app_id`, `WM_CLASS`).

L'écran « Paramètres » écrit dans le même fichier (`theme`, `accent`,
`[dock.screens]`) ; le dock le relit quand le compositeur l'en prévient.

Le dock **réécrit ce fichier** quand on change à la souris les favoris
(épingler, retirer, réordonner), le thème ou la taille du lanceur : seule la ligne concernée
change, les commentaires restent. Un fichier qui n'est pas du TOML valide
n'est jamais réécrit.

## Ce que fait la souris

- **Barre** : elle montre les favoris, puis toutes les autres applis
  ouvertes, quel que soit leur écran (barre d'accent : ouverte sur l'écran
  du dock ; point gris : ouverte ailleurs). Clic gauche lance ou montre la fenêtre utilisée en dernier (un
  nouveau clic passe à la suivante, une fenêtre réduite revient) ; clic
  milieu ouvre une nouvelle fenêtre ; clic droit ouvre le menu (ouvrir sur un
  écran, nouvelle fenêtre, les actions du fichier `.desktop` de l'appli, cinq au plus
  — « Nouvelle fenêtre de navigation privée » —, épingler, oublier la place, fermer) ; glisser un favori le déplace.
- **Tiroir** : clic sur le lanceur ou touche `Super` seule (`Alt` en
  imbriqué). La saisie filtre aussitôt, sur le nom des applis puis sur ce qu'elles sont et leurs mots-clés (`GenericName` et `Keywords` de leur fichier `.desktop`, dans la langue de la session : « navigateur » trouve Firefox, « terminal » kitty), les flèches choisissent, `Entrée`
  ouvre, `Échap` ou un clic en dehors ferme ; la molette sur la grille change
  de page. Tirer sa poignée le redimensionne : le bord du tiroir vertical
  opposé au dock (`[drawer] width`, jamais moins de 520 px ni au point de ne plus
  laisser de place au mixeur), le bord haut du lanceur horizontal
  (`[drawer] height`, jamais moins de 580 px, qui gardent la place du
  mixeur sous le centre de contrôle, ni plus haut que l'écran moins 200 px). Pendant qu'on la tire, la poignée
  (`ui/grip.rs`) ne donne la position du pointeur qu'une fois par image : chaque
  message refait la vue de tous les panneaux, et une souris envoie jusqu'à
  mille mouvements par seconde. La pagination montre cinq numéros au plus, autour de la
  page affichée.
  La barre qu'il redessine par-dessus la vraie répond comme elle (clics,
  menu, glisser un favori, zone de notification), sans se déployer ni nommer
  l'icône survolée ; ce qui lance ou montre une appli ferme le lanceur, et
  les menus s'ouvrent par-dessus lui.
- **Centre de contrôle** : bascules, volume et luminosité (molette sur un
  curseur pour le régler ; les touches de luminosité d'un portable la font
  aussi varier de 5 %, même en jeu) ; sur un portable, la batterie sous le
  nom de l'utilisateur ; la flèche au bout du volume ouvre le mixeur (dans
  le lanceur horizontal, il est toujours affiché sous le centre de
  contrôle), seul endroit où choisir la sortie audio. Le bouton aux
  curseurs, à côté de la déconnexion, ouvre l'écran « Paramètres »
  (`coquille-settings`, voir `crates/settings/README.md`) : c'est là que se
  règle le sens du dock sur chaque écran, que les barres suivent aussitôt.

## Comment c'est fait

- `model/` : toutes les règles (barre, tiroir, mixeur), sans interface,
  testées. La crate `coquille-apps` lit les fichiers `.desktop` et trouve les
  icônes, dans le thème Reversal qui va avec le thème et l'accent (le
  compositeur s'en sert aussi, pour ses barres de titre),
  `steam.rs` lit la ludothèque, `config.rs` lit et réécrit la configuration.
  Applis et jeux sont relus quand on en installe ou en retire
  (`backend/installed.rs` : une empreinte des dossiers toutes les 3 s, la
  relecture une fois qu'elle ne bouge plus) ; le compositeur relit les siens
  quand s'ouvre la fenêtre d'une appli qu'il ne connaît pas.
- `ui/` : les vues, avec [iced](https://iced.rs) et `iced_layershell`.
- `backend/` : le lien avec le compositeur (crate `coquille-ipc`) et les
  services. Le son passe par `pactl` et `parec` (l'interface PulseAudio de
  PipeWire), le réseau par `nmcli`, le Bluetooth par `bluetoothctl`, la
  luminosité par le rétroéclairage d'un portable (`backend/backlight.rs` :
  lu dans `/sys/class/backlight`, réglé par logind avec `busctl`, relu toutes
  les deux secondes) sinon par `ddcutil`, la batterie par `upower`
  (`backend/battery.rs` : relue à chaque ligne de `upower --monitor` et
  chaque minute). Un outil absent laisse sa commande grisée.
- **Batterie** (portable seulement) : au-dessus de l'heure dans la barre
  verticale, avant elle dans la barre horizontale, avec le temps restant
  dans la barre dépliée et le centre de contrôle. À 10 % puis 5 % sur
  batterie, une notification du dock lui-même prévient ; la dernière passe
  « Ne pas déranger » (pas un écran ancré : pendant un jeu, elle attend dans
  l'historique).
- Couleurs, symboles, lien au compositeur et police Figtree (licence OFL,
  embarquée dans le binaire) viennent de la crate `coquille-ui`, partagée
  avec l'écran « Paramètres ».
- Les panneaux du lanceur retiennent les clics avec leur propre `opaque`
  (`ui/opaque.rs`) : celui d'iced demande le curseur `Idle`, dont
  `iced_layershell` 0.19.1 fait le curseur d'attente.

## Notifications

Le dock sert `org.freedesktop.Notifications` sur le bus de session
(`backend/notify.rs`) ; les règles sont dans `model/notify.rs`, testées.

- **Bulles** : des capsules de la matière, de l'épaisseur et de l'arrondi
  de la barre (`ui/toasts.rs`), dans une surface layer-shell à part
  (`coquille-toasts`, calque `Overlay`). Elles se posent dans le coin bas de
  l'écran de la première barre visible (gauche, puis centre, puis droite), à
  la marge du dock : **sous une barre verticale** (coin gauche, ou droit pour
  une barre à droite), **à droite d'une barre horizontale**, sur sa ligne.
  Elles s'empilent vers le haut, la plus récente dans le coin ; trois au
  plus, et sous une barre verticale seulement celles qui tiennent sans la
  toucher. Icône à la place de celles du dock (image de la notification,
  sinon son icône, sinon celle de l'appli), résumé, une ligne du corps sans
  balises, deux boutons d'action au plus, croix. Seules les capsules
  prennent le pointeur, pas l'ombre autour.
- **Délai** : celui de l'appli, 5 s par défaut ; `0` ou l'urgence critique
  la gardent jusqu'à ce qu'on s'en occupe. Une bulle qui expire disparaît
  mais **la notification reste ouverte**, dans l'historique (le serveur
  annonce `persistence`) ; seule une notification `transient` est fermée
  avec elle.
- **Historique** : les notifications encore ouvertes ont leur section dans
  le lanceur, à gauche du centre de contrôle et de sa hauteur : une carte
  par notification, la plus récente en haut (icône, résumé, appli et
  ancienneté, deux lignes du corps, deux actions au plus, croix), le reste
  en défilement ; une cloche et « Aucune notification » quand il n'y en a
  pas. Un clic sur une
  carte ou sur une bulle déclenche son action `default` s'il y en a une et
  amène la fenêtre de l'appli (trouvée par `desktop-entry`, sinon par le nom
  de l'appli), puis la ferme. La croix la ferme, « Tout effacer » les ferme
  toutes. Une appli qui ferme sa notification la retire aussi. Rien n'est
  gardé d'une session à l'autre.
- **Ne pas déranger** : la bascule du centre de contrôle retire les bulles,
  et un écran ancré par un jeu fait de même ; les notifications arrivent
  quand même dans l'historique. Les bulles se retirent aussi tant que le
  lanceur est ouvert.
- **Nom de bus** : dans une session coquille (`XDG_CURRENT_DESKTOP`), le
  dock prend le nom, quitte à passer devant un serveur resté là s'il le
  permet, ou à attendre son tour. Imbriqué dans un autre bureau, il ne prend
  qu'un nom libre : les notifications de l'hôte ne sont jamais détournées.
  Pour essayer en imbriqué, lancer coquille sous `dbus-run-session` avec
  `--backends compositor,notifications` pour le dock, puis `notify-send`
  (ou `gdbus call`, qui n'attend pas les actions).

## Limites

- **Dock horizontal** : sur un écran de moins de 1550 px, la colonne des
  jeux disparaît (ils restent dans la recherche), puis
  notifications et centre de contrôle rétrécissent jusqu'à 280 px.
- **Avec `--mock`**, une seule barre s'affiche (celle du premier écran
  configuré) : les écrans simulés n'existent pas pour de vrai.

- **Luminosité** : sur un PC fixe, il faut `ddcutil` et l'accès aux bus
  `i2c` des écrans ; sans eux le curseur est remplacé par « indisponible ».
  Sur un portable, seul l'écran intégré est réglé (pas un écran externe).
- **Éclairage nocturne** retient sa position mais n'a pas d'effet : il n'y a
  pas encore d'éclairage nocturne.
- **Notifications** : ni son, ni réponse dans la bulle, ni jeton
  d'activation (`ActivationToken`) : le dock amène lui-même la fenêtre de
  l'appli, ce qui suffit tant qu'elle en a une ouverte. Une capsule ne montre
  qu'une ligne du corps. Avec
  `--mock`, pas de notifications.
- **Icônes X11 de la zone de notification** (Wine, Proton : Ubisoft
  Connect…) : le dock est la zone de notification XEmbed du serveur X
  (`backend/xembed.rs`). Elles n'ont pas d'info-bulle (Wine n'en dit rien
  à X11). Les clics sont rejoués directement sur la fenêtre de l'icône :
  une icône GTK sous X11, qui ne croit que les clics venus du serveur, ne
  réagit pas. Si le dock tombe, Wine replie ses icônes dans sa petite
  fenêtre, puis les redonne au dock relancé mais laisse cette fenêtre
  affichée, vide (défaut de Wine) : la fermer suffit.
- **Messages non lus** : seule la simulation en montre.
- **Jeux** : la ludothèque Steam seulement. Un jeu est reconnu par la classe
  de ses fenêtres (`steam_app_…`) ; un jeu natif lancé hors de la rangée Jeux
  s'ouvre comme une application ordinaire.
- **Niveaux du mixeur** : un petit `parec` par application tant que le mixeur
  est ouvert ; rien ne tourne quand il est fermé.
- Le son d'un jeu lancé par Proton porte le nom de son exécutable, pas
  toujours celui du jeu.
- Glisser une icône du tiroir vers un écran n'est pas réalisé (le menu
  « Ouvrir sur » de la barre le remplace).
- Les textes sont plus grands que sur les maquettes, où ils feraient 8 à
  10 px à l'échelle réelle : les noms longs sont coupés dans la grille.
