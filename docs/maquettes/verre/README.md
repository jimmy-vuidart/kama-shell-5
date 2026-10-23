# Captures de référence — Verre liquide

Ce dossier regroupe les captures de référence de la refonte « verre liquide »,
en thème clair et en thème sombre, ainsi que les images d'inspiration d'origine
dans [`references/`](references/).

## Contenu

- **Bureau** (`bureau_dark.png`, `bureau_light.png`) : deux fenêtres rangées
  dans leurs emplacements avec leurs capsules au-dessus, et une fenêtre
  flottante au centre avec sa barre de titre en verre, son ombre douce et sa
  barre de rangement (retirée depuis).
- **Dock déployé** (`dock_dark.png`, `dock_light.png`) : la barre latérale en
  verre avec le survol déployant les libellés.
- **Tiroir / Lanceur** (`tiroir_dark.png`, `tiroir_light.png`) : le tiroir
  déplié, le centre de contrôle en verre et ses curseurs.
- **Mixeur** (`mixeur_dark.png`, `mixeur_light.png`) : le mixeur de volume avec
  fond en verre dépoli et contours rim.
- **Paramètres** (`parametres_dark.png`, `parametres_light.png`) : la fenêtre
  de réglages avec son arrière-plan flouté par le compositeur et ses cartes
  translucides.

## Comment les reproduire

Toutes les captures se font en mode imbriqué (`--role left` ou `--three-screens`)
avec les options `--screenshot <fichier>` et `--debug-input <script>`.

### 1. Bureau (deux fenêtres rangées et une flottante)

Préparer un état temporaire `state/memory.json` :
```json
{
  "term1": { "role": "left", "slot": 0 },
  "term2": { "role": "left", "slot": 1 }
}
```

Puis lancer :
```bash
cargo run -p coquille -- --role left --config config.toml --state-dir state/ \
  --spawn "kitty --app-id term1" \
  --spawn "kitty --app-id term2" \
  --spawn "kitty --app-id term3" \
  --screenshot bureau.png --screenshot-delay 3000
```

### 2. Dock déployé

```bash
cargo run -p coquille -- --role left --config config.toml \
  --spawn "target/debug/coquille-dock --mock" \
  --debug-input "1200 move 20 450" \
  --screenshot dock.png --screenshot-delay 2500
```

### 3. Tiroir (lanceur)

```bash
cargo run -p coquille -- --role left --config config.toml \
  --spawn "target/debug/coquille-dock --mock" \
  --debug-input "1200 key alt" \
  --screenshot tiroir.png --screenshot-delay 2500
```

### 4. Mixeur audio

```bash
cargo run -p coquille -- --role left --config config.toml \
  --spawn "target/debug/coquille-dock --mock --mixer" \
  --screenshot mixeur.png --screenshot-delay 2000
```

### 5. Paramètres

```bash
cargo run -p coquille -- --role left --config config.toml \
  --spawn "target/debug/coquille-settings --config config.toml --page appearance" \
  --screenshot parametres.png --screenshot-delay 2500
```
