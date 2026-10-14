# coquille-settings

L'écran « Paramètres » de coquille : une fenêtre ordinaire (pas un panneau),
que le compositeur ouvre, range et mémorise comme n'importe quelle appli.

Elle règle `~/.config/coquille/config.toml`. Chaque changement est **écrit
aussitôt**, puis le compositeur relit le fichier, applique ce qui peut
changer en cours de session et prévient le dock : il n'y a pas de bouton
« Appliquer ».

## Lancer

Dans coquille, par le bouton aux curseurs du centre de contrôle du dock (à
côté de la déconnexion), ou depuis le tiroir une fois installé.

À la main, en imbriqué, sur un fichier d'essai :

```bash
cargo build -p coquille -p coquille-dock -p coquille-settings
```

```bash
cp config/config.toml /tmp/essai.toml
```

```bash
target/debug/coquille --role left --config /tmp/essai.toml --spawn "$PWD/target/debug/coquille-dock --backends compositor --config /tmp/essai.toml" --spawn "$PWD/target/debug/coquille-settings --config /tmp/essai.toml"
```

Sans `--config`, les trois programmes lisent et écrivent le vrai fichier.

| Option | Effet |
|---|---|
| `--config FICHIER` | Fichier de configuration (défaut : `~/.config/coquille/config.toml`). |
| `--page PAGE` | Page d'ouverture : `appearance`, `screens`, `game`, `mouse`, `system`. |

## Les pages

| Page | Réglages | Prend effet |
|---|---|---|
| Apparence | thème clair ou sombre, couleur d'accent (celle du thème, huit pastilles, ou `#rrggbb` saisi) | aussitôt |
| Écrans et dock | connecteur de chaque écran | **au prochain démarrage** |
| | dock de chaque écran : à gauche, à droite, en bas, aucun | aussitôt |
| Mode jeu | bords collants et leur délai, affichage direct, fréquence variable | aussitôt |
| Souris | vitesse du pointeur, profil d'accélération, défilement naturel, inversion des boutons | aussitôt |
| Système | disposition et variante du clavier, terminal | aussitôt |
| | applications X11 (XWayland) | **au prochain démarrage** |

- Donner à un écran le connecteur d'un autre **échange** les deux : deux
  écrans ne partagent jamais un connecteur.
- En imbriqué, les écrans sont simulés et leurs noms ne sont pas des
  connecteurs : la liste est remplacée par « Écrans simulés ».
- Lancée hors de coquille (pas de `COQUILLE_SOCKET`), la fenêtre règle le
  fichier ; tout s'applique au prochain démarrage.
- Les favoris du dock, les épinglées et la taille du lanceur ne sont pas
  ici : ils se règlent à la souris, dans le dock.

## Le fichier

L'écriture est faite par la crate `coquille-config`, comme pour le dock :

- seule la ligne concernée change, les commentaires restent ;
- un réglage que le fichier ne fait que suggérer en commentaire
  (`# vrr = true`) est activé sur place, sous son explication ;
- revenir à une valeur par défaut (« Celle du thème », « Celle du système »,
  « Automatique ») retire la ligne ;
- un fichier qui n'est pas du TOML valide n'est jamais réécrit : un bandeau
  le dit, rien n'est enregistré tant qu'il n'est pas corrigé.

## Comment c'est fait

- `model.rs` : ce que l'écran propose, sans interface et testé — les pages,
  les lignes de chaque liste (dispositions lues dans
  `/usr/share/X11/xkb/rules/evdev.lst`, terminaux trouvés dans le `PATH`),
  le dessin des trois écrans.
- `app.rs` : l'état et ce qui le change ; `view.rs` : les vues, avec iced.
- Couleurs, police et lien au compositeur viennent de `coquille-ui`, comme
  pour le dock : la fenêtre suit le thème et l'accent en cours.

## Limites

- Les dispositions du clavier portent leur nom anglais, celui de XKB.
- La disposition donnée à coquille par `--keyboard-layout` l'emporte sur le
  fichier.
- Le rôle des écrans ne change pas en cours de session.
