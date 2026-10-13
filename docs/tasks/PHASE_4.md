# Phase 4 — Shell

La phase 4 est le **dock** : barre, tiroir d'applications, centre de contrôle
et mixeur, sur le bord gauche de l'écran gauche. Cahier des charges :
`docs/maquettes/dock.md` ; maquettes : `docs/maquettes/dock/`.

Les phases 0 à 3 sont terminées (voir `CLAUDE.md`, sections « État de la
phase 2 » et « État de la phase 3 »).

## Étapes du dock

- [x] 1. Layer-shell dans le compositeur (panneaux, zone réservée, retrait
      sur un écran ancré), outil de test complété (`rclick`, `mclick`,
      `wheel`).
- [x] 2. Modèles du dock et messages de la crate `ipc`.
- [x] 3. Dock en rendu statique avec `--mock`.
- [x] 4. Canal du shell côté compositeur, règles du modèle, thème modifiable,
      accent, sections du dock acceptées dans la configuration.
- [x] 5. Dock interactif : survol, clics, menu contextuel, réordonnancement.
- [x] 6. Tiroir : recherche, épinglées, applications paginées, jeux Steam.
- [x] 7. Centre de contrôle et mixeur avec les services réels. Les actions
      de session, d'abord simulées, sont réelles depuis que Jimmy l'a demandé.
- [x] 8. Intégration à la session : `scripts/install.sh` installe
      `coquille-dock` et `coquille-dock-watch`, `coquille-session` lance le
      dock avec le compositeur et le relance s'il tombe (journal :
      `dock.log`). Essayée en imbriqué et dans un dossier d'essai ; **reste à
      lancer `scripts/install.sh` et à ouvrir la session pour de vrai**.
- [x] 8 bis. Écran « Paramètres » (`coquille-settings`, crate `settings`) :
      une vraie fenêtre, ouverte par le bouton du centre de contrôle, qui
      règle `config.toml` et le fait relire à chaud. Essayé en imbriqué.
- [ ] 9. Optionnel : messages non lus réels (`com.canonical.Unity.LauncherEntry`),
      glisser une icône du tiroir vers un écran.

Tout ce qui est coché a été vérifié en imbriqué et par tests ; rien n'a
encore tourné sur le backend matériel. Détail de ce qui n'a pas été exercé :
`CLAUDE.md`, « État de la phase 4 ».

## Décisions prises avec le dock

- `Super` seul ouvre le tiroir et libère le pointeur d'un jeu ;
  `Super+Échap` remet le jeu en fenêtre.
- Le bouton « ajouter un onglet » des barres de titre est supprimé.
- Les onglets quittent les barres de titre pour des **capsules** au-dessus
  de chaque emplacement ; l'échange avec le centre (⇄, `Mod+C`) et la
  disposition « Onglets » sont supprimés. Détail :
  `docs/superpowers/specs/2026-10-12-capsules-design.md`.
- Chaque barre montre toutes les applis ouvertes, pas seulement celles
  de son écran.
- Un dock par écran si on le veut, vertical (bord gauche ou droit) ou
  horizontal (bord bas), réglé dans `[dock.screens]` ; le lanceur horizontal
  est un panneau centré, jamais sur toute la largeur.
- Se déconnecter, Redémarrer et Éteindre sont réels, après confirmation ;
  seul `--mock` les simule.
- Les favoris et le thème changés à la souris sont réécrits dans
  `config.toml`, commentaires conservés.
- La police Figtree est embarquée dans le binaire du dock.
- Vraies icônes du thème système, pas les carrés colorés des maquettes.
- « Jeux au centre » : reconnus à leur classe `steam_app_*`, plus tout
  lancement depuis la rangée Jeux. **Jugé insuffisant par Jimmy, accepté pour
  le moment** : les jeux natifs lancés autrement ne sont pas reconnus.
- « Éclairage nocturne » et « Ne pas déranger » : bascules sans effet.

## Ce que le dock reprend, et ce qui reste sans support

| Fonction | Avec le dock |
|---|---|
| Lancer une appli | tiroir, favoris |
| Retrouver une fenêtre réduite | clic sur son icône dans le dock |
| Voir les fenêtres ouvertes | toutes, des trois écrans (favoris puis « OUVERTES »), avec leur écran |
| Heure, réseau, son | dock et centre de contrôle |
| Bascule « ne pas déranger » | centre de contrôle, sans effet |
| Thème, sens du dock, clavier, mode jeu | écran « Paramètres » |
| Changer la disposition d'un écran | **toujours rien à la souris** (`Super+1` à `5`) |
| « Tout envoyer vers l'écran… » | **toujours rien** |
| Afficher les notifications | **toujours rien** (concept conservé) |
| Enregistrer et restaurer un instantané | **toujours rien** (concept conservé) |

## Conservé comme concept, design à faire

### Notifications

- Le compositeur reçoit et affiche les notifications des applications.
- **Mode jeu** : elles ne s'affichent jamais sur l'écran d'un jeu ancré, elles
  vont sur un écran latéral.
- **Ne pas déranger** automatique pendant le jeu.

### Instantanés

- Mémoriser la disposition actuelle des trois écrans, la restaurer.
- Optionnels : rien n'oblige à préparer quoi que ce soit.

## Reports qui ne dépendent pas du design

- [ ] Les fenêtres sans aucune barre de titre n'ont pas de moyen de
      fermeture à la souris hors du dock (son menu « Fermer la fenêtre » le
      fait pour les applis qu'il montre) : un raccourci « fermer la fenêtre
      active ».
- [x] Les barres de titre et les onglets montrent le vrai nom et la vraie
      icône de l'appli (crate `apps`, partagée avec le dock).
- [x] `scripts/install.sh` dit « déjà installée » quand les entrées de
      l'écran de connexion existent.

## Hors de la phase 4

- Rangés dans « Plus tard » par la feuille de route : le calque par-dessus le
  jeu (performances, son par appli, capture) et les vignettes vivantes.
- Sans phase attribuée : le halo en secouant la souris, le branchement à
  chaud des écrans, la sélection primaire (clic du milieu), le
  glisser-déposer entre applis X11 et Wayland.
