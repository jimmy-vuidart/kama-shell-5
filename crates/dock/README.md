# coquille-dock

Le dock de coquille : une barre fine sur le bord d'un écran, le lanceur
d'applications qu'elle ouvre, son centre de contrôle et son mixeur par
application. Un seul programme, plusieurs surfaces layer-shell, un seul état.

Chaque écran peut avoir sa barre, dans un sens ou dans l'autre :

- **verticale** : colonne sur le bord gauche ou sur le bord droit, qui se
  déploie au survol ; le lanceur est un tiroir sur toute la hauteur, du même
  côté. À droite tout est en miroir : les icônes restent contre le bord, les
  noms se déploient vers la gauche, le menu et le mixeur s'ouvrent à gauche ;
- **horizontale** : rangée centrée sur le bord bas, qui nomme l'icône
  survolée dans une étiquette ; le lanceur est un panneau large et centré
  au-dessus d'elle, en trois colonnes.

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
| `--mock` | Simule tout : fenêtres, son, réseau, Bluetooth, luminosité, session. |
| `--backends a,b,…` | Les services réels à utiliser, les autres étant simulés : `compositor`, `audio`, `network`, `bluetooth`, `brightness`, `session`. Sans l'option : tous. |
| `--output NOM` | L'écran où s'afficher. Par défaut, celui que le compositeur désigne comme écran gauche. |
| `--config FICHIER` | Fichier de configuration (défaut : `~/.config/coquille/config.toml`). |
| `--state-dir DOSSIER` | Où garder les volumes par application (défaut : `~/.local/state/coquille`, et un dossier temporaire avec `--mock`). |
| `--drawer` | Ouvre le tiroir dès le démarrage (pour une capture). |

`RUST_LOG=coquille_dock=debug` détaille ce que fait le dock.

**Session.** Se déconnecter, Redémarrer et Éteindre demandent toujours une
confirmation, puis agissent pour de vrai : la déconnexion demande au
compositeur de quitter, le redémarrage et l'extinction passent par
`systemctl reboot` et `systemctl poweroff`. Si le système refuse, sa raison
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
height = 500           # lanceur d'un dock horizontal
```

Sans `[dock.screens]`, il y a un dock vertical sur l'écran gauche et aucun
ailleurs. Toutes les barres montrent les mêmes applis ; « Ici » y désigne
l'écran de la barre. La touche `Super` ouvre le lanceur sur l'écran du
pointeur s'il a un dock, sinon sur le premier écran qui en a un.

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
  écran, épingler, oublier la place, fermer) ; glisser un favori le déplace.
- **Tiroir** : clic sur le lanceur ou touche `Super` seule (`Alt` en
  imbriqué). La saisie filtre aussitôt, les flèches choisissent, `Entrée`
  ouvre, `Échap` ou un clic en dehors ferme ; la molette sur la grille change
  de page. Tirer sa poignée le redimensionne : le bord du tiroir vertical
  opposé au dock (`[drawer] width`, jamais moins de 520 px ni au point de ne plus
  laisser de place au mixeur), le bord haut du lanceur horizontal
  (`[drawer] height`, jamais moins de 480 px).
- **Centre de contrôle** : bascules, volume et luminosité (molette sur un
  curseur pour le régler) ; la flèche au bout du volume ouvre le mixeur, seul
  endroit où choisir la sortie audio. Le bouton aux
  curseurs, à côté de la déconnexion, ouvre l'écran « Paramètres »
  (`coquille-settings`, voir `crates/settings/README.md`) : c'est là que se
  règle le sens du dock sur chaque écran, que les barres suivent aussitôt.

## Comment c'est fait

- `model/` : toutes les règles (barre, tiroir, mixeur), sans interface,
  testées. La crate `coquille-apps` lit les fichiers `.desktop` et trouve les
  icônes, dans le thème Reversal qui va avec le thème et l'accent (le
  compositeur s'en sert aussi, pour ses barres de titre),
  `steam.rs` lit la ludothèque, `config.rs` lit et réécrit la configuration.
- `ui/` : les vues, avec [iced](https://iced.rs) et `iced_layershell`.
- `backend/` : le lien avec le compositeur (crate `coquille-ipc`) et les
  services. Le son passe par `pactl` et `parec` (l'interface PulseAudio de
  PipeWire), le réseau par `nmcli`, le Bluetooth par `bluetoothctl`, la
  luminosité par `ddcutil`. Un outil absent laisse sa commande grisée.
- Couleurs, symboles, lien au compositeur et police Figtree (licence OFL,
  embarquée dans le binaire) viennent de la crate `coquille-ui`, partagée
  avec l'écran « Paramètres ».
- Les panneaux du lanceur retiennent les clics avec leur propre `opaque`
  (`ui/opaque.rs`) : celui d'iced demande le curseur `Idle`, dont
  `iced_layershell` 0.19.1 fait le curseur d'attente.

## Limites

- **Dock horizontal** : le lanceur fait au plus 1100 px de large, moins sur
  un écran étroit ; sa grille a trois colonnes de six lignes. Les bascules
  du centre de contrôle y vont deux par deux.
- **Avec `--mock`**, une seule barre s'affiche (celle du premier écran
  configuré) : les écrans simulés n'existent pas pour de vrai.

- **Luminosité** : il faut `ddcutil` et l'accès aux bus `i2c` des écrans ;
  sans eux le curseur est remplacé par « indisponible ».
- **Ne pas déranger** et **Éclairage nocturne** retiennent leur position mais
  n'ont pas d'effet : il n'y a encore ni notifications ni éclairage nocturne.
- **Icônes X11 de la zone de notification** (Wine, Proton : Ubisoft
  Connect…) : elles passent par `xembedsniproxy` (paquet `plasma-workspace`),
  que le compositeur lance quand XWayland est prêt. Sans lui, Wine les
  montre dans une petite fenêtre à lui. Un clic droit ouvre le menu que
  l'appli dessine elle-même, en haut à gauche de l'écran gauche : le pont ne
  sait pas où est l'icône. Une icône GTK sous X11 ne reçoit pas les clics
  (le pont les rejoue par XTest, là où XWayland croit le pointeur).
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
