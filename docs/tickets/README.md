# Tickets

Les tâches issues de la revue technique du 13 octobre 2026, faite sur le
commit `1ad7eaa`. Les numéros de ligne cités dans les tickets sont ceux de ce
commit.

La revue s'est faite à la lecture du code : rien n'a été exécuté sur le
backend matériel. Chaque ticket dit donc ce qui est **lu dans le code** et ce
qui reste **à confirmer** par un essai.

Un ticket terminé est supprimé dans le commit qui le règle (l'historique git
en garde la trace), et sa ligne retirée de ce tableau.

## Priorité haute

| N° | Ticket | Nature | Se vérifie |
|---|---|---|---|
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
| 11 | [Les voyants du clavier ne s'allument pas](11-voyants-clavier.md) | bug | matériel |
| 12 | [Aucun périphérique d'entrée n'est réglé](12-reglages-libinput.md) | évolution | matériel |
| 13 | [Une fenêtre démappée sans être détruite reste en fantôme](13-fenetre-demappee.md) | bug | imbriqué |
| 14 | [Une fenêtre refusée par `manage` n'est jamais configurée](14-echec-de-manage.md) | bug | tests |
| 15 | [Les habitudes n'ont ni version ni copie de secours](15-habitudes-version.md) | robustesse | tests |
| 16 | [Le canal du shell n'a pas de version](16-version-du-canal.md) | dette | tests |
| 18 | [Le dock lance trop de processus](18-processus-du-dock.md) | performance | imbriqué |
| 19 | [Une appli installée en cours de session n'a ni nom ni icône](19-catalogue-a-chaud.md) | bug | imbriqué |

## Architecture et évolutions

| N° | Ticket | Nature | Se vérifie |
|---|---|---|---|
| 20 | [Sortir la cadence des images en logique pure](20-cadence-en-logique-pure.md) | dette | tests |
| 22 | [Mise en veille des écrans](22-veille-des-ecrans.md) | évolution | matériel |
| 23 | [Verrouillage de la session](23-verrouillage-de-session.md) | évolution | imbriqué |
| 24 | [Protocole `wp_presentation`](24-presentation-time.md) | évolution | matériel |

## Refonte « verre liquide »

Les huit tickets de la refonte visuelle (verre translucide et flouté, sans
changer aucune disposition) sont dans [`glass/`](glass/README.md), avec leurs
images de référence, leurs dépendances et l'ordre où les confier à des
agents.

## Ordre conseillé

1. **20 puis 01**, et **02** : le confort en jeu. Le 20 rend le 01 testable
   sans le matériel.
2. **07, 09, 10** : courts, sans dépendance.
3. **05**, qui profite des drapeaux par écran du 01, puis **06**.
4. **03 et 04** : demandent des essais sur le matériel avec Jimmy.
5. Le reste selon l'envie ; **22 à 24** attendent des décisions de Jimmy.
