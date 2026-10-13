# 18 — Le dock lance trop de processus

- **Priorité :** faible
- **Nature :** performance
- **Zone :** `crates/dock/src/backend/audio.rs`, `services.rs`
- **Se vérifie :** en imbriqué
- **Certitude :** lu dans le code ; coût non mesuré

## Constat

Le dock passe par les outils des services plutôt que par leurs bibliothèques.
C'est une décision du projet, que ce ticket ne remet pas en cause. Trois
endroits en abusent :

1. **Réseau et Bluetooth, toutes les cinq secondes** (`services.rs:139` et
   `175`) : deux `nmcli` et deux `bluetoothctl` à chaque tour, que le centre
   de contrôle soit ouvert ou non. Environ 70 000 processus par jour.
2. **Son : trois `pactl` par événement** (`read`, `audio.rs:128`). Chaque
   ligne de `pactl subscribe` qui parle d'une sortie ou d'un flux déclenche
   une relecture complète. Les événements arrivent par rafales ; ils sont
   traités un par un, sans regroupement.
3. **Curseur de volume : un `pactl` par cran** (`audio.rs:144`). Les demandes
   s'empilent dans un canal non borné et sont exécutées l'une après l'autre,
   chacune suivie d'une relecture. Le son suit le curseur avec du retard.

## Conséquence

Des réveils inutiles en permanence, et un mixeur qui traîne quand on tire un
curseur ou quand plusieurs flux démarrent ensemble.

## Piste

1. Réseau et Bluetooth : lire l'état à l'ouverture du centre de contrôle,
   puis toutes les cinq secondes tant qu'il est ouvert ; au repos, une
   lecture par minute suffit aux indicateurs de la barre. `nmcli monitor`
   permettrait de s'en passer pour le réseau.
2. Son : après un événement, vider les lignes déjà arrivées avant de relire,
   ou attendre quelques dizaines de millisecondes de calme.
3. Volume : ne garder que la dernière valeur demandée pour une même cible.

## Terminé quand

- Dock au repos, tiroir fermé : au plus quelques processus par minute
  (comptés avec `strace -f -e trace=execve`, ou `bpftrace`).
- Tirer le curseur de volume d'un bout à l'autre : le son suit sans retard
  perceptible, et le nombre de `pactl` lancés est de l'ordre de la durée du
  geste, pas du nombre de crans.

## Liens

- Voisin de : 07 (même fichier).
