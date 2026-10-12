# Phase 4 — Shell

**En attente.** La phase 4 dépend d'un nouveau design de lanceur, qui reste à
définir. Tant qu'il n'existe pas, il n'y a pas de plan de réalisation : ce
fichier garde seulement ce qui est décidé, ce qui est conservé comme idée, et
les petits reports qui ne dépendent de rien.

Les phases 0 à 3 sont terminées (voir `CLAUDE.md`, sections « État de la
phase 2 » et « État de la phase 3 »).

## Abandonné

- Les **barres latérales** telles que dessinées dans les maquettes.
- Le **lanceur** tel que dessiné dans les maquettes.
- Le **saut de curseur** au bouton latéral de la souris.

Les maquettes de `docs/maquettes/` ne font donc plus référence pour ces
éléments.

## Conservé comme concept, design à refaire

Ces deux idées restent voulues. Leur forme (où elles s'affichent, comment on
les déclenche, comment elles sont construites) dépendra du nouveau lanceur ;
rien n'est à réaliser avant.

### Notifications

- Le compositeur reçoit et affiche les notifications des applications.
- **Mode jeu** : elles ne s'affichent jamais sur l'écran d'un jeu ancré, elles
  vont sur un écran latéral.
- **Ne pas déranger** automatique pendant le jeu.

### Instantanés

- Mémoriser la disposition actuelle des trois écrans, la restaurer.
- Optionnels : rien n'oblige à préparer quoi que ce soit.

## À définir avec le nouveau lanceur

Ces fonctions devaient vivre dans les barres ou le lanceur abandonnés. Elles
gardent leur solution actuelle, ou n'en ont pas, jusqu'à ce que le nouveau
design dise où elles vont :

| Fonction | Aujourd'hui |
|---|---|
| Lancer une appli | `Super+Entrée` ouvre un terminal |
| Changer la disposition d'un écran | `Super+1` à `6` |
| Retrouver une fenêtre réduite | son onglet grisé, ou `Super+R` |
| Voir les fenêtres d'un écran | rien |
| « Tout envoyer vers l'écran… » | rien |
| Heure, réseau, son | rien |
| Notifications, « ne pas déranger » | rien |
| Enregistrer et restaurer un instantané | rien |

Le socle technique (protocole layer-shell pour des panneaux séparés, canal
entre le compositeur et les programmes du shell, lecture des fichiers
`.desktop` pour les vrais noms et icônes des applis) sera dimensionné à ce
moment-là, selon ce que le design demande.

## Reports qui ne dépendent pas du design

Faisables dès maintenant, indépendamment du lanceur :

- [ ] Les fenêtres sans aucune barre de titre n'ont pas de moyen de
      fermeture : un raccourci « fermer la fenêtre active ».
- [ ] L'icône « ajouter un onglet » se lit comme un dossier : à redessiner.
- [ ] Le message « écran de connexion : non installée » de
      `scripts/install.sh` est trompeur quand les entrées existent déjà.

## Hors de la phase 4

- Rangés dans « Plus tard » par la feuille de route : le calque par-dessus le
  jeu (performances, son par appli, capture) et les vignettes vivantes.
- Sans phase attribuée : le halo en secouant la souris, le branchement à
  chaud des écrans, la sélection primaire (clic du milieu), le
  glisser-déposer entre applis X11 et Wayland.
