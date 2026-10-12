# coquille-dock

Le dock de coquille : une barre fine sur le bord gauche de l'écran gauche,
le tiroir d'applications qu'elle ouvre, le centre de contrôle en pied de
tiroir et son mixeur par application. Un seul programme, plusieurs surfaces
layer-shell, un seul état.

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

## Options

| Option | Effet |
|---|---|
| `--mock` | Simule tout : fenêtres, son, réseau, Bluetooth, luminosité, session. |
| `--backends a,b,…` | Les services réels à utiliser, les autres étant simulés : `compositor`, `audio`, `network`, `bluetooth`, `brightness`, `session`. Sans l'option : tous sauf `session`. |
| `--output NOM` | L'écran où s'afficher. Par défaut, celui que le compositeur désigne comme écran gauche. |
| `--config FICHIER` | Fichier de configuration (défaut : `~/.config/coquille/config.toml`). |
| `--state-dir DOSSIER` | Où garder les volumes par application (défaut : `~/.local/state/coquille`, et un dossier temporaire avec `--mock`). |
| `--drawer` | Ouvre le tiroir dès le démarrage (pour une capture). |

`RUST_LOG=coquille_dock=debug` détaille ce que fait le dock.

**Session.** Se déconnecter, Redémarrer et Éteindre demandent toujours une
confirmation, et restent **simulés** (journalisés, rien n'est fait) tant que
`session` ne figure pas dans `--backends`.

## Configuration

Dans `~/.config/coquille/config.toml`, partagé avec le compositeur (modèle :
`config/config.toml`) :

```toml
theme = "dark"
accent = "#7aa7ff"

[dock]
favorites = ["firefox", "discord", "kitty", "code", "obsidian"]

[drawer]
pinned = ["firefox", "discord", "kitty", "code", "obsidian", "mpv", "org.gnome.Nautilus"]
```

Un nom est celui d'un fichier `.desktop` (sans l'extension), sa dernière
partie (`nautilus` pour `org.gnome.Nautilus`), le programme qu'il lance, ou
ce que les fenêtres annoncent (`app_id`, `WM_CLASS`).

Le dock **réécrit ce fichier** quand on change les favoris à la souris
(épingler, retirer, réordonner) ou le thème : seule la ligne concernée
change, les commentaires restent. Un fichier qui n'est pas du TOML valide
n'est jamais réécrit.

## Ce que fait la souris

- **Barre** : clic gauche lance ou montre la fenêtre utilisée en dernier (un
  nouveau clic passe à la suivante, une fenêtre réduite revient) ; clic
  milieu ouvre une nouvelle fenêtre ; clic droit ouvre le menu (ouvrir sur un
  écran, épingler, oublier la place, fermer) ; glisser un favori le déplace.
- **Tiroir** : clic sur le lanceur ou touche `Super` seule (`Alt` en
  imbriqué). La saisie filtre aussitôt, les flèches choisissent, `Entrée`
  ouvre, `Échap` ou un clic en dehors ferme ; la molette sur la grille change
  de page.
- **Centre de contrôle** : bascules, volume et luminosité (molette sur un
  curseur pour le régler), « Par appli » ouvre le mixeur.

## Comment c'est fait

- `model/` : toutes les règles (barre, tiroir, mixeur), sans interface,
  testées. `catalog.rs` lit les fichiers `.desktop`, `steam.rs` la
  ludothèque, `config.rs` lit et réécrit la configuration.
- `ui/` : les vues, avec [iced](https://iced.rs) et `iced_layershell`.
- `backend/` : le lien avec le compositeur (crate `coquille-ipc`) et les
  services. Le son passe par `pactl` et `parec` (l'interface PulseAudio de
  PipeWire), le réseau par `nmcli`, le Bluetooth par `bluetoothctl`, la
  luminosité par `ddcutil`. Un outil absent laisse sa commande grisée.
- La police Figtree (licence OFL, `assets/fonts/`) est embarquée dans le
  binaire.

## Limites

- **Luminosité** : il faut `ddcutil` et l'accès aux bus `i2c` des écrans ;
  sans eux le curseur est remplacé par « indisponible ».
- **Ne pas déranger** et **Éclairage nocturne** retiennent leur position mais
  n'ont pas d'effet : il n'y a encore ni notifications ni éclairage nocturne.
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
