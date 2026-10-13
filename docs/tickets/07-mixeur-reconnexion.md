# 07 — Le mixeur meurt si PipeWire redémarre

- **Priorité :** moyenne
- **Nature :** robustesse
- **Zone :** `crates/dock/src/backend/audio.rs`
- **Se vérifie :** en imbriqué
- **Certitude :** lu dans le code

## Constat

Le service audio du dock écoute `pactl subscribe`. Quand ce processus se
termine, le service journalise « the audio server hung up » et fait `return`
(`audio.rs:251`). Le flux de l'abonnement iced est alors fini, et iced ne
relance pas un abonnement terminé.

## Conséquence

Après un redémarrage de PipeWire (mise à jour, `systemctl --user restart
pipewire`, plantage), le volume, la sourdine et le mixeur du dock ne
répondent plus jusqu'au redémarrage du dock.

## Piste

Entourer le service d'une boucle de reconnexion, comme le lien au compositeur
(`crates/ui/src/compositor.rs`) : attendre une seconde, relancer
`pactl subscribe`, relire l'état en entier. Le canal des demandes (`requests`)
doit survivre à la reconnexion, sinon le dock garde un émetteur mort.

L'absence de `pactl` (`audio.rs:209`) reste un abandon définitif : l'outil
n'apparaîtra pas en cours de session.

## Terminé quand

- Essai en imbriqué, dock sans `--mock` : `systemctl --user restart pipewire
  pipewire-pulse`, puis le curseur de volume du dock règle toujours le son et
  le mixeur liste toujours les applis.
- Le journal du dock dit la coupure et la reprise, une fois chacune.

## Liens

- Voisin de : 18 (même fichier).
