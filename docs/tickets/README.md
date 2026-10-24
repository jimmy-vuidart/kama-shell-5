# Tickets

Les tâches précises, une par fichier : bugs, dette, évolutions sans phase.
Les grands chantiers sont dans les documents de phase (`docs/phase-5.md`),
qui renvoient aux tickets quand ils en ont un.

Les tickets 01 à 20 viennent de la revue technique du 13 octobre 2026, faite
à la lecture du code sur le commit `1ad7eaa` : leurs numéros de ligne sont
ceux de ce commit et ont pu bouger depuis (`scripts/codegraph.sh` retrouve
les symboles). Chaque ticket dit ce qui est **lu dans le code** et ce qui
reste **à confirmer** par un essai.

Un ticket terminé est supprimé dans le commit qui le règle (l'historique git
en garde la trace), et sa ligne retirée de ce tableau. Les numéros ne sont
jamais réutilisés : le suivant est le 33.

## Faits, à vérifier sur le matériel

| N° | Ticket | Nature |
|---|---|---|
| 11 | [Les voyants du clavier ne s'allument pas](11-voyants-clavier.md) | bug |
| 12 | [Aucun périphérique d'entrée n'est réglé](12-reglages-libinput.md) | évolution |

## Priorité haute

| N° | Ticket | Nature | Se vérifie |
|---|---|---|---|
| 30 | [Essayer coquille sur le portable](30-portable-sur-le-materiel.md) | vérification | portable |
| 01 | [Une image vide bloque l'écran pendant une période](01-cadence-image-vide.md) | performance | matériel |
| 02 | [Les fenêtres cachées ne reçoivent pas de « frame callback »](02-frame-callbacks-fenetres-cachees.md) | bug | imbriqué |
| 03 | [Un écran éteint puis rallumé reste noir](03-evenements-de-connecteur.md) | robustesse | matériel |
| 04 | [XWayland n'est jamais relancé](04-relance-xwayland.md) | robustesse | imbriqué |

## Priorité moyenne

| N° | Ticket | Nature | Se vérifie |
|---|---|---|---|
| 05 | [`apply_arrangement` fait tout et redessine tout](05-apply-arrangement.md) | dette, performance | imbriqué |
| 06 | [Plusieurs tests de survol par événement souris](06-tests-de-survol.md) | performance | imbriqué |
| 07 | [Le mixeur meurt si PipeWire redémarre](07-mixeur-reconnexion.md) | robustesse | imbriqué |
| 08 | [`gsettings` bloque le rendu du compositeur](08-gsettings-bloquant.md) | robustesse | imbriqué |
| 09 | [L'écriture de `config.toml` peut détruire le fichier](09-ecriture-config.md) | bug | tests |
| 10 | [Les programmes lancés par le compositeur restent en zombies](10-processus-zombies.md) | fuite | imbriqué |

## Priorité faible

| N° | Ticket | Nature | Se vérifie |
|---|---|---|---|
| 13 | [Une fenêtre démappée sans être détruite reste en fantôme](13-fenetre-demappee.md) | bug | imbriqué |
| 14 | [Une fenêtre refusée par `manage` n'est jamais configurée](14-echec-de-manage.md) | bug | tests |
| 15 | [Les habitudes n'ont ni version ni copie de secours](15-habitudes-version.md) | robustesse | tests |
| 16 | [Le canal du shell n'a pas de version](16-version-du-canal.md) | dette | tests |
| 18 | [Le dock lance trop de processus](18-processus-du-dock.md) | performance | imbriqué |
| 26 | [Pas de raccourci pour fermer la fenêtre active](26-fermer-la-fenetre-active.md) | évolution | imbriqué |
| 27 | [Halo autour du curseur quand on secoue la souris](27-halo-du-curseur.md) | évolution | imbriqué |
| 28 | [Dock : messages non lus réels, glisser une icône vers un écran](28-dock-non-lus-et-glisser.md) | évolution | imbriqué |
| 29 | [Seuls les jeux Steam sont reconnus comme jeux](29-jeux-reconnus.md) | évolution | tests |
| 31 | [Gestes du pavé tactile](31-gestes-du-pave.md) | évolution | portable |
| 32 | [Un écran externe branché au portable](32-ecran-externe-du-portable.md) | évolution | portable |

## Architecture et évolutions

| N° | Ticket | Nature | Se vérifie |
|---|---|---|---|
| 20 | [Sortir la cadence des images en logique pure](20-cadence-en-logique-pure.md) | dette | tests |
| 22 | [Mise en veille des écrans](22-veille-des-ecrans.md) (phase 5, étape 4) | évolution | matériel |
| 23 | [Verrouillage de la session](23-verrouillage-de-session.md) (phase 5, étape 4) | évolution | imbriqué |
| 24 | [Protocole `wp_presentation`](24-presentation-time.md) | évolution | matériel |
| 25 | [Installer coquille comme paquet AUR](25-paquet-aur.md) | évolution | chroot, session |

## Ordre conseillé

1. **20 puis 01**, et **02** : le confort en jeu. Le 20 rend le 01 testable
   sans le matériel.
2. **07, 09, 10** : courts, sans dépendance.
3. **05**, qui profite des drapeaux par écran du 01, puis **06**.
4. **03 et 04**, et la vérification de **11 et 12** : demandent des essais
   sur le matériel avec Jimmy.
5. Le reste selon l'envie ; **22 et 23** avec la phase 5 ; **26 à 29**
   attendent une décision de Jimmy (touche, geste, indices) ; **25** quand
   coquille sera prête à être distribuée.
