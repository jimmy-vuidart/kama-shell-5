# 14 — Une fenêtre refusée par `manage` n'est jamais configurée

- **Priorité :** faible
- **Nature :** bug (cas d'erreur)
- **Zone :** `crates/compositor/src/shell.rs`
- **Se vérifie :** par relecture et tests
- **Certitude :** lu dans le code ; aucun cas réel connu

## Constat

`map_window` (`shell.rs:460`) retire le toplevel de `shell.unmapped`, puis
appelle `manage` (`shell.rs:491`). `manage` peut rendre `None` à trois
endroits :

1. aucun écran où ouvrir la fenêtre ;
2. le modèle refuse la fenêtre (`open_window` en erreur) ;
3. la fenêtre vient d'être acceptée par le modèle mais n'apparaît pas dans
   `arrangement()` (le `?` après `find`).

Dans les trois cas, la `Window` est abandonnée sans configure.

## Conséquence

- Le client attend indéfiniment son configure initial : il ne s'affiche
  jamais et ne le sait pas.
- Dans le troisième cas, le modèle garde une fenêtre que le shell ne connaît
  pas : capsule sans fenêtre, focus sur rien.

Aucun de ces cas ne devrait arriver aujourd'hui. Ils deviennent possibles le
jour où un écran peut disparaître (ticket 03).

## Piste

- Cas 1 et 2 : demander au client de fermer (`send_close`) et le dire dans le
  journal, plutôt que le laisser attendre.
- Cas 3 : défaire l'ouverture dans le modèle (`close_window`) avant de
  sortir.

## Terminé quand

- Les trois sorties de `manage` laissent le modèle et le shell d'accord sur
  les fenêtres qui existent.
- Aucune ne laisse un client sans réponse.

## Liens

- Voisin de : 13, 03.
