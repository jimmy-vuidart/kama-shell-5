# 09 — L'écriture de `config.toml` peut détruire le fichier

- **Priorité :** moyenne
- **Nature :** bug (perte de données)
- **Zone :** `crates/config/src/edit.rs`
- **Se vérifie :** par tests unitaires
- **Certitude :** lu dans le code

## Constat

`update` (`edit.rs:181`) lit le fichier, applique un changement, écrit à côté
puis déplace par-dessus. Trois défauts :

1. **Toute erreur de lecture vaut « fichier vide ».**
   `read_to_string(path).unwrap_or_default()` (`edit.rs:182`) ne distingue pas
   « absent » de « illisible » : droits, erreur disque, ou fichier qui n'est
   pas de l'UTF-8 valide (un commentaire accentué enregistré en Latin-1). Le
   changement est alors appliqué à un texte vide, et le fichier remplacé par
   ce seul réglage.
2. **Un lien symbolique est remplacé par un fichier ordinaire.** `rename`
   écrase le lien, pas sa cible. Un `config.toml` géré par stow, chezmoi ou un
   dépôt de dotfiles est détaché de sa source sans rien dire.
3. **Deux programmes écrivent sans se coordonner.** Le dock (favoris, thème,
   taille du lanceur) et l'écran « Paramètres » lisent, modifient et écrivent
   chacun de leur côté, avec le même nom de fichier temporaire
   (`config.toml.tmp`, `edit.rs:193`). Le dernier qui écrit efface le
   changement de l'autre.

## Conséquence

Le premier défaut détruit la configuration entière, commentaires compris. Le
deuxième casse des dotfiles. Le troisième est peu probable (une seule souris)
mais silencieux.

## Piste

1. N'accepter que `NotFound` comme fichier vide ; toute autre erreur est
   rendue à l'appelant, et rien n'est écrit.
2. Résoudre le chemin (`fs::canonicalize`) avant d'écrire : le fichier
   temporaire va à côté de la cible réelle, et c'est elle qui est remplacée.
3. Un nom temporaire propre au processus, et un verrou consultatif (`flock`
   sur un fichier voisin) autour de lire-modifier-écrire.

## Terminé quand

Tests unitaires dans `edit.rs` :

- un fichier qui n'est pas de l'UTF-8 valide est laissé tel quel, et `update`
  rend une erreur ;
- un fichier illisible (droits retirés) aussi ;
- un lien symbolique reste un lien, et sa cible reçoit le changement ;
- deux `update` qui changent deux réglages différents, l'un après l'autre à
  partir de la même lecture, laissent les deux changements.

## Liens

- Aucun.
