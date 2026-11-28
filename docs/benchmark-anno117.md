# Protocole de benchmark : Coquille vs GNOME (Anno 117)

Ce protocole vise à mesurer de façon reproductible l'impact du compositeur (**Coquille** vs **GNOME / Mutter**) sur les performances d'Anno 117 sur la station principale (RTX 4070 Ti SUPER, 3 écrans avec centre 21:9 `DP-2` à 3440×1440).

## 1. Métriques mesurées

1. **Performance et régularité du jeu (MangoHud, `log_interval=0`)** :
   - **FPS** : moyen, médian, **1 % Low**, **0.1 % Low**.
   - **Frametime (ms)** : médian, P95, P99, P99.9, écart-type.
   - **Stutters** : nombre et pourcentage de frames dépassant `2 × médiane` et `16.67 ms` / `33.33 ms`.
2. **Coût du compositeur (`monitor-session.sh`)** :
   - CPU % et RAM (RSS MiB) de `coquille` (+ `coquille-dock`) vs `gnome-shell`.
   - CPU % et RAM de `Xwayland` (si le jeu tourne via Proton/XWayland).
   - Horloges GPU, utilisation GPU %, VRAM utilisée, puissance (W) et température via `nvidia-smi`.
3. **Fonctionnement du mode jeu Coquille (`session.log`)** :
   - Activation effective du **Direct Scanout** (ou bascule en composition si échec).
   - Activation effective du **VRR** sur `DP-2`.

---

## 2. Prérequis et contrôle des variables

Pour que la comparaison soit valide, **tout doit être identique entre GNOME et Coquille**, hormis le compositeur :

- **Même version de Proton** et mêmes options graphiques dans Anno 117 (pas de changement de qualité, DLSS/FSR, ou limite de FPS entre les sessions).
- **Synchronisation verticale (V-Sync) désactivée** dans le jeu (et pas de limiteur de FPS artificiel si l'on veut mesurer le débit brut ; ou bien un plafond identique si l'on mesure uniquement la stabilité/latence).
- **Même sauvegarde et même scène exacte** :
  - Charger une sauvegarde lourde (grande ville développée, beaucoup d'unités/bâtiments).
  - Attendre **3 à 5 minutes** après le lancement pour que les horloges GPU/CPU et les shaders soient stabilisés (chauffe préalable, hors mesure).
  - Définir une **séquence de test fixe de 60 secondes** : par exemple 30 s caméra immobile sur le centre-ville dense, puis 30 s de panoramique/zoom reproductible le long d'un axe fixe.
- **Gouverneur CPU identique** dans les deux sessions (vérifier `cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor`).
- **Ordre alterné (ABBA)** pour annuler la dérive thermique :
  - Exemple : 2 runs sous GNOME → 2 runs sous Coquille → 2 runs sous Coquille → 2 runs sous GNOME (ou 3 runs par scénario, en conservant la médiane).

---

## 3. Installation rapide dans Steam

1. Ouvrir les **Propriétés** d'Anno 117 dans Steam → **Options de lancement** :
   ```text
   /home/jimmy/Projets/kama-shell-5/scripts/bench/launch-anno117.sh %command%
   ```
2. Raccourci MangoHud configuré par défaut : **`Shift_L + F2`** démarre un enregistrement automatique de **60 secondes** (et s'arrête tout seul).
   - Un premier appui lance la capture dans `~/benchmarks/anno117/<session>/<horodatage>/`.
   - Vous pouvez enchaîner 3 captures de 60 s dans la même partie (attendre quelques secondes entre chaque).

---

## 4. Scénarios de test

### Scénario S1 — Jeu seul (meilleur cas)
- **Setup** : Anno 117 en plein écran sur l'écran central (`DP-2`, 3440×1440). Aucune autre fenêtre ouverte sur `DP-1` et `HDMI-A-1`.
- **But** : Mesurer l'overhead minimal de Coquille (Direct Scanout + VRR) face à GNOME/Mutter.
- **Runs** : 3 × 60 s sous GNOME, puis 3 × 60 s sous Coquille.

### Scénario S2 — Jeu + Compagnons (usage réel 3 écrans)
- **Setup** : Anno 117 en plein écran au centre (`DP-2`), **Discord** à gauche (`DP-1`) et **navigateur avec une vidéo YouTube 1080p60 ou page animée** à droite (`HDMI-A-1`).
- **But** : Vérifier que le rendu actif sur les écrans latéraux ne dégrade pas le *frame pacing* (1 % / 0.1 % low) du jeu au centre.
- **Runs** : 3 × 60 s sous GNOME, puis 3 × 60 s sous Coquille.

### Scénario S3 — Isolation du Direct Scanout (Coquille uniquement, optionnel)
- **Setup** : Identique à S1, mais avec `direct_scanout = false` dans `[game]` de `~/.config/coquille/config.toml` (puis remis à `true` après le test).
- **But** : Quantifier précisément le gain apporté par le *direct scanout* sur NVIDIA dans Coquille par rapport au chemin composité.

---

## 5. Analyse des résultats

Une fois les captures terminées dans les deux sessions, lancer l'analyseur depuis la racine du dépôt :

```bash
python3 scripts/bench/analyze-bench.py \
  --baseline ~/benchmarks/anno117/gnome \
  --target ~/benchmarks/anno117/coquille \
  --markdown ~/benchmarks/anno117/rapport.md
```

Le script affiche :
- Le tableau comparatif complet (FPS moyen/médian/1%/0.1%, frametimes P95/P99/P99.9, stutters, deltas en %).
- La consommation CPU/RAM moyenne de `gnome-shell` vs `coquille` et de `Xwayland`.
- Les métriques GPU (`nvidia-smi`) pour vérifier que les fréquences/puissances étaient comparables.
- Le statut **Direct Scanout** et **VRR** extrait de `~/.local/state/coquille/session.log` pour chaque run Coquille.
