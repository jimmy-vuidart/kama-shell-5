# 23 — Verrouillage de la session

- **Priorité :** haute
- **Nature :** fonction manquante
- **Zone :** `crates/compositor` (protocole, rendu, entrées), `crates/dock`
- **Se vérifie :** en imbriqué
- **Certitude :** absence lue dans le code

## Constat

Rien ne permet de verrouiller la session : ni raccourci, ni bouton, ni le
protocole `ext-session-lock-v1` qu'utilisent les écrans de verrouillage
(swaylock, hyprlock…).

## Conséquence

Pour s'absenter sans laisser la session ouverte, il faut se déconnecter, donc
fermer toutes les fenêtres.

## Décidé avec la phase 5

Le verrouillage fait partie de la session autonome (`docs/phase-5.md`,
étape 4) : par le protocole, avec hyprlock ou swaylock d'abord, un écran de
verrouillage à nous plus tard ; verrouillage avant la mise en veille.

## À décider avec Jimmy

- Comment verrouiller à la main : un bouton du centre de contrôle à côté de
  la déconnexion, un raccourci ?

## Piste

Par le protocole, qui laisse le choix du programme :

1. Offrir `ext-session-lock-v1` (Smithay le fournit).
2. Tant que le verrou tient : chaque écran ne montre que la surface du
   verrou (ou du noir avant qu'elle arrive), et toutes les entrées vont à
   elle. Aucun raccourci du shell ne passe, sauf le changement de terminal
   virtuel.
3. Si le programme du verrou meurt sans déverrouiller, la session **reste**
   verrouillée : c'est la règle du protocole, à ne pas contourner.

## Terminé quand

- Essai scripté en imbriqué avec un écran de verrouillage : les fenêtres ne
  sont plus visibles sur aucun écran, les clics et les touches ne les
  atteignent pas, le déverrouillage rend tout tel quel.
- Tuer le programme du verrou ne déverrouille pas.

## Liens

- Va naturellement avec : 22.
- Étape 4 de la phase 5.
