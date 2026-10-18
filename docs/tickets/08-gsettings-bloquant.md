# 08 — `gsettings` bloque le rendu du compositeur

- **Priorité :** moyenne
- **Nature :** robustesse
- **Zone :** `crates/compositor/src/decoration/icons.rs`, `crates/apps/src/icons.rs`
- **Se vérifie :** en imbriqué
- **Certitude :** lu dans le code ; gel à confirmer

## Constat

Le thème d'icônes est demandé à `gsettings` par un sous-processus synchrone
(`coquille_apps::icon_theme`, `apps/src/icons.rs:7`). Le compositeur
l'appelle à la première icône dessinée (`decoration/icons.rs:25`), c'est-à-dire
depuis `refresh_chrome`, pendant la préparation d'une image.

Dans le même chemin, pour chaque nouvelle icône : recherche dans les dossiers
du thème, lecture du fichier, rendu du SVG (`decoration/icons.rs:27`).

Le compositeur n'a qu'un fil : pendant ce temps, rien n'est dessiné et aucune
entrée n'est traitée.

**Depuis le 17 octobre 2026** : le thème d'icônes est Reversal
(`coquille_apps::icon_theme_for`, `apps/src/reversal.rs`). Quand il est
installé, `gsettings` n'est plus appelé : le compositeur lit seulement
quelques dossiers d'icônes à la première barre dessinée et à chaque
changement de thème ou d'accent (`IconCache::follow`). `gsettings` reste le
repli sans Reversal, et la recherche et le rendu de chaque icône restent dans
le chemin de rendu.

## Conséquence

- Si D-Bus ou dconf ne répond pas (début de session, session lancée depuis
  un TTY), `gsettings` peut attendre son délai maximal : l'écran gèle à
  l'ouverture de la première fenêtre.
- En temps normal : un à-coup à la première fenêtre, et un plus petit à la
  première fenêtre de chaque appli.

## Piste

1. Demander le thème au démarrage, dans un fil à part ; le résultat revient
   par un canal calloop. En attendant, les titres montrent le carré de
   couleur, puis sont repeints.
2. Borner l'attente (une ou deux secondes) et retomber sur `Adwaita` puis
   `hicolor`, ce que fait déjà `icon_theme` quand la réponse est vide.
3. Faire de même pour la recherche et le rendu des icônes : hors du fil
   principal, repeindre à l'arrivée.

Le dock charge déjà ses icônes dans `spawn_blocking` (`dock/src/app.rs:307`) :
il n'est pas concerné.

## Terminé quand

- Avec un faux `gsettings` qui dort trente secondes placé en tête du `PATH`,
  le compositeur imbriqué ouvre une fenêtre sans geler, et son titre reçoit
  son icône au plus tard quand le délai tombe.
- Aucun `Command::output` ni lecture de dossier dans le chemin de rendu.

## Liens

- Voisin de : 19 (catalogue des applis).
- La phase 5 compte ce repli parmi les dépendances à un autre bureau.
