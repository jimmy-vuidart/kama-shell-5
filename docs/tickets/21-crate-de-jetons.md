# 21 — Une crate de jetons d'apparence sans iced

- **Priorité :** architecture
- **Nature :** dette (duplication)
- **Zone :** `crates/ui`, `crates/compositor/src/theme.rs`, `decoration/paint.rs`
- **Se vérifie :** par tests unitaires, puis captures en imbriqué
- **Certitude :** lu dans le code

## Constat

La crate `ui` mêle deux choses : les valeurs de l'apparence (couleurs, tons
de l'or, recettes des dégradés) et leur mise en œuvre pour iced. Comme elle
dépend d'iced, le compositeur ne peut pas en dépendre. Il réécrit donc les
mêmes valeurs :

- `Accent` existe deux fois : `crates/ui/src/style.rs:38` et
  `crates/compositor/src/theme.rs:60` ;
- chaque programme a sa table de gris chauds, écrite à part
  (`crates/ui/src/style.rs:149`, `crates/compositor/src/theme.rs:226`) : pas
  les mêmes surfaces, mais une même famille que rien ne relie dans le code ;
- les recettes du métal (anneau, pièce fine, lingot) sont écrites deux fois :
  `crates/ui/src/metal.rs:39` pour iced, `crates/compositor/src/theme.rs:165`
  pour tiny-skia et le shader ;
- la couleur de repli d'une appli a déjà divergé (ticket 17).

`CLAUDE.md` nomme lui-même les deux emplacements de `Accent` : la duplication
est connue, pas encore traitée.

## Conséquence

Tout changement d'apparence se fait à deux endroits, et rien ne signale un
oubli. Le brief de l'or (`docs/gold/`) dit que ses valeurs font foi : elles
devraient n'exister qu'une fois dans le code.

## Piste

Une petite crate sans dépendance graphique (`coquille-theme`, par exemple) :

- les gris des deux thèmes, en octets, ceux du compositeur comme ceux du
  dock ;
- les onze tons de l'or et leur variante du thème clair ;
- les recettes des dégradés, comme listes d'arrêts (position, couleur) ;
- les mesures communes (rayons, hauteurs, espacements) qui ne sont pas déjà
  dans `model::Metrics` ;
- la couleur de repli d'une appli.

`ui` convertit ces valeurs en types d'iced ; le compositeur, en `Rgba` et en
uniformes de shader. Ce qu'iced impose (le biseau cuit dans le dégradé, les
filets en boîtes) reste dans `ui/metal.rs`.

## Terminé quand

- Chaque couleur et chaque recette n'est écrite qu'une fois.
- Les captures en imbriqué (clair et sombre, compositeur, dock et
  « Paramètres ») sont identiques avant et après.
- `CLAUDE.md` est mis à jour : l'arbre des crates, et la phrase qui cite les
  deux emplacements de `Accent`.

## Liens

- Règle au passage : 17.
