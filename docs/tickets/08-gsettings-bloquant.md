# 08 — Les icônes des barres de titre sont chargées pendant le rendu

- **Priorité :** faible
- **Nature :** robustesse, performance
- **Zone :** `crates/compositor/src/decoration/icons.rs`, `decoration/paint.rs`, `crates/apps/src/icons.rs`
- **Se vérifie :** en imbriqué
- **Certitude :** lu dans le code ; à-coups non mesurés

## Constat

Le compositeur n'a qu'un fil : ce qui s'y passe pendant la préparation d'une
image retarde le dessin et les entrées. Deux choses s'y font encore.

1. **Chaque icône est cherchée et dessinée à sa première demande.**
   `IconCache::get` (`decoration/icons.rs`) est appelé par le peintre
   (`decoration/paint.rs` : barre de titre, capsules, sélecteur de
   fenêtres), donc depuis `refresh_chrome`. Pour un couple (nom, taille)
   pas encore connu, il cherche le fichier dans le thème
   (`coquille_apps::find_icon`, `freedesktop_icons`), le lit et le dessine
   (`resvg` ou PNG). Le résultat est gardé, mais tout est oublié quand le
   thème d'icônes change. Arrivent donc dans le chemin de rendu :
   - la première fenêtre de chaque appli ;
   - le premier `Alt+Tab`, qui demande toutes les icônes à une autre taille ;
   - un changement d'échelle (nouvelle taille en pixels) ;
   - un changement de thème ou d'accent qui change de variante de Reversal :
     toutes les icônes affichées sont redessinées dans la même image.
2. **Sans Reversal, `gsettings` est appelé de façon synchrone.**
   `IconCache::follow` prend le thème d'icônes par
   `coquille_apps::icon_theme_for` (`apps/src/reversal.rs`), qui lit quelques
   dossiers puis, quand aucune variante de Reversal n'est installée, se replie
   sur `icon_theme` (`apps/src/icons.rs`) : un `Command::output` de
   `gsettings`. C'est fait à la première image décorée et à chaque changement
   de thème ou d'accent.

Déjà réglé : avec Reversal installé (vérifié par `scripts/install.sh`,
dépendance du futur paquet, ticket 25), `gsettings` n'est plus appelé. Le
dock n'est pas concerné : il charge ses icônes dans `spawn_blocking`
(`dock/src/app.rs`).

## Conséquence

- Avec Reversal : un petit à-coup à la première fenêtre de chaque appli, au
  premier `Alt+Tab` et quand le thème change. Quelques millisecondes par
  icône, à mesurer ; gênant surtout si cela tombe pendant qu'un jeu tourne
  sur un autre écran.
- Sans Reversal, si D-Bus ou dconf ne répond pas (session lancée depuis un
  TTY, début de session) : `gsettings` peut attendre son délai maximal, et
  l'écran gèle à l'ouverture de la première fenêtre décorée. Cas devenu
  marginal, d'où la priorité faible.

## Piste

1. Chercher et dessiner les icônes dans un fil à part (comme `app_theme.rs`
   le fait pour `gsettings`), le résultat revenant par un canal calloop. En
   attendant, la barre montre le carré de couleur qui sert déjà quand une
   appli n'a pas d'icône, puis elle est repeinte à l'arrivée.
2. Dans ce même fil, choisir le thème d'icônes (`icon_theme_for`), et
   borner l'attente de `gsettings` (une ou deux secondes) avant de retomber
   sur `Adwaita` puis `hicolor`, comme le fait déjà `icon_theme` quand la
   réponse est vide.
3. Éventuellement, préparer à l'avance les icônes des fenêtres ouvertes à la
   taille du sélecteur, pour que le premier `Alt+Tab` n'attende rien.

## Terminé quand

- Aucun `Command::output`, lecture de fichier ou rendu de SVG dans le chemin
  de rendu (relecture de `refresh_chrome` et de ce qu'il appelle).
- Sans Reversal, avec un faux `gsettings` qui dort trente secondes placé en
  tête du `PATH` : le compositeur imbriqué ouvre une fenêtre sans geler, et
  son titre reçoit son icône au plus tard quand le délai tombe.
- Les captures des essais scriptés existants sont identiques une fois les
  icônes arrivées.

## Liens

- Voisin de : 25 (Reversal devient une dépendance du paquet).
- La phase 5 (`docs/phase-5.md`) compte le repli `gsettings` parmi les
  dépendances à un autre bureau, de priorité faible.
