# Documentation

`CLAUDE.md`, à la racine, est la référence : le projet, ses règles, ses
décisions et l'état de chaque phase. Ce dossier en garde le détail.

| Où | Quoi |
|---|---|
| [`phase-5.md`](phase-5.md) | La phase en cours : une session qui se passe de Plasma et de GNOME |
| [`tickets/`](tickets/README.md) | Les tâches précises : bugs, dette, évolutions sans phase |
| [`conception/`](conception/) | Ce qui a été conçu et réalisé, et fait encore référence : [capsules](conception/capsules.md), [dock](conception/dock.md) |
| [`maquettes/`](maquettes/README.md) | Les maquettes et les captures de référence, et ce qui n'y est plus vrai |

Ailleurs dans le dépôt : `crates/dock/README.md` et `crates/settings/README.md`
(lancer, options, limites de chaque programme), `vendor/README.md` (code
tiers), `config/config.toml` (modèle commenté de la configuration).

## Règles de rangement

- Une phase terminée n'a plus de document : ce qui reste vrai passe dans
  `CLAUDE.md`, ce qui reste à faire devient un ticket.
- Un plan d'implémentation sert le temps de la réalisation, puis est
  supprimé (l'historique git le garde). Seule la spécification, mise à jour,
  rejoint `conception/`.
- Un document décrit l'état actuel, pas l'historique des changements.
- Noms de fichiers en minuscules, sans espaces ni accents.
