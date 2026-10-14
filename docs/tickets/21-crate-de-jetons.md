# 21 — Une crate de jetons d'apparence sans iced

- **Priorité :** architecture
- **Nature :** dette (duplication)
- **Zone :** `crates/ui`, `crates/compositor/src/theme.rs`, `decoration/paint.rs`
- **Se vérifie :** par tests unitaires, puis captures en imbriqué
- **Certitude :** lu dans le code

## Constat

La crate `ui` mêle deux choses : les valeurs de l'apparence (couleurs,
accent par défaut de chaque thème) et leur mise en œuvre pour iced. Comme elle
dépend d'iced, le compositeur ne peut pas en dépendre. Il réécrit donc les
mêmes valeurs :

- l'accent par défaut de chaque thème est écrit deux fois :
  `crates/ui/src/style.rs:60` et `crates/compositor/src/theme.rs:91` ;
- chaque programme a sa table de gris, écrite à part (`crates/ui/src/style.rs`,
  `crates/compositor/src/theme.rs`) : pas les mêmes surfaces, mais une même
  famille que rien ne relie dans le code ;
- la lecture d'une couleur `#rrggbb` (le réglage `accent`) existe de chaque
  côté ;
- la couleur de repli d'une appli a déjà divergé (ticket 17).

## Conséquence

Tout changement d'apparence se fait à deux endroits, et rien ne signale un
oubli.

## Piste

Une petite crate sans dépendance graphique (`coquille-theme`, par exemple) :

- les gris des deux thèmes, en octets, ceux du compositeur comme ceux du
  dock ;
- l'accent par défaut de chaque thème, et la lecture de `#rrggbb` ;
- les mesures communes (rayons, hauteurs, espacements) qui ne sont pas déjà
  dans `model::Metrics` ;
- la couleur de repli d'une appli.

`ui` convertit ces valeurs en types d'iced ; le compositeur, en `Rgba`.

## Terminé quand

- Chaque couleur n'est écrite qu'une fois.
- Les captures en imbriqué (clair et sombre, compositeur, dock et
  « Paramètres ») sont identiques avant et après.
- `CLAUDE.md` est mis à jour : l'arbre des crates.

## Liens

- Règle au passage : 17.
