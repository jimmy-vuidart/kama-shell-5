# 08 — Documentation et captures finales

- **Priorité :** troisième vague, après tous les autres
- **Nature :** documentation et contrôle
- **Zone :** `CLAUDE.md`, `docs/maquettes/dock.md`, `docs/tickets/README.md`, `docs/tickets/glass/`
- **Se vérifie :** relecture, captures en imbriqué
- **Certitude :** —

Lire d'abord [le README de la refonte](README.md).

## But

Clore la refonte : la documentation dit ce qui est vrai, un jeu de captures
sert de référence, et ce qui reste à essayer sur le matériel est noté pour
Jimmy.

## Piste

1. **`CLAUDE.md`** (ne modifier que lui : `AGENTS.md` en est un lien) :
   - **Apparence** : l'ancienne description « surfaces translucides, sobre »
     devient le **verre liquide** ;
   - **accent par défaut** : `#5b63e6` en clair, `#8f97ff` en sombre, au lieu
     de `#2f6fed` et `#7aa7ff` ;
   - la **règle des alphas**, et le fait que le compositeur floute derrière
     les programmes `coquille-*` ;
   - la section **`[glass]`** (`blur`, `strength`) dans la liste des réglages
     de `config.toml` ;
   - la règle cardinale de la refonte, comme décision : le verre n'a changé
     aucune disposition ;
   - **Architecture** : la crate de jetons (ticket 01) dans l'arbre ; dans les
     décisions techniques, `render/glass.rs` (framebuffer effects de Smithay,
     Kawase double, masques, `unsafe` documenté) et les ombres ;
   - **Backend matériel** : le flou et les ombres jamais essayés sur les vrais
     écrans (voir point 4).
2. **`docs/maquettes/dock.md`**, section « Apparence » (lignes 125-155) :
   - les couleurs d'aplat deviennent les jetons de verre ;
   - la mention « flou non exigé » devient « flou fourni par le compositeur ».
3. **Captures de référence** dans `docs/maquettes/verre/`, en clair et en
   sombre :
   - le bureau avec deux fenêtres rangées et une flottante ;
   - le dock déployé ;
   - le tiroir ;
   - le mixeur ;
   - les Paramètres.

   On y ajoute un court `README.md` qui dit comment les refaire (commandes
   `--screenshot` et `--debug-input`).
4. **Contrôle du mode jeu**, en imbriqué : `kitty --start-as fullscreen` sur un
   écran, puis on vérifie :
   - aucun `GlassElement` ni ombre sur l'écran ancré (`debug!` des tickets
     04 et 05) ;
   - les autres écrans gardent leur verre.
5. **À essayer avec Jimmy sur le matériel**, à noter dans `CLAUDE.md`
   (« Fait mais jamais exécuté ») :
   - coût du flou sur les trois écrans (3440×1440 au centre) ;
   - affichage direct toujours obtenu en jeu (`direct scanout: the buffer of the game is shown as it is` dans `session.log`) ;
   - `--probe` sans erreur.
6. **Tickets** :
   - `docs/tickets/glass/` est supprimé une fois tous les tickets faits, en
     gardant les images de `references/` dans `docs/maquettes/verre/` ;
   - sa ligne est retirée de `docs/tickets/README.md`.

## Terminé quand

- `CLAUDE.md` et `docs/maquettes/dock.md` décrivent le verre tel qu'il est.
- Les captures de référence sont dans `docs/maquettes/verre/`.
- Le contrôle du mode jeu est fait et noté dans le commit.
- `docs/tickets/glass/` n'existe plus ; les images de référence sont
  conservées.

## Liens

- Dépend de : 01 à 07.
