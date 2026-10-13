# 10 — Les programmes lancés par le compositeur restent en zombies

- **Priorité :** moyenne
- **Nature :** fuite (entrées de la table des processus)
- **Zone :** `crates/compositor/src/state.rs`
- **Se vérifie :** en imbriqué
- **Certitude :** lu dans le code

## Constat

`State::spawn` (`state.rs:377`) lance `sh -c <commande>` et abandonne le
`Child` rendu par `spawn()`. Rust n'attend pas un enfant abandonné, et le
compositeur n'ignore pas `SIGCHLD` : quand le programme se termine, il reste
à l'état de zombie jusqu'à la fin du compositeur.

Concernés : le terminal de `Mod+Entrée`, et les commandes de `--spawn`.

## Conséquence

Un zombie de plus par terminal ouvert au clavier, pendant toute la session.
Bénin (une entrée de table, pas de mémoire), mais c'est la seule vraie fuite
trouvée par la revue, et elle se voit dans `ps`.

## Piste

Garder les `Child` dans `State` et les récolter avec `try_wait` : à chaque
nouveau lancement, et sur un minuteur lent.

**Deux pièges :**

- Ne pas reprendre telle quelle la technique du dock (`sh -c '… &'`, puis
  attendre le shell, `dock/src/app.rs:1450`). `coquille-dock-watch` surveille
  le compositeur par `$PPID` : détaché, il aurait `init` pour parent et
  s'arrêterait aussitôt.
- Ne pas récolter « n'importe quel enfant » (`waitpid(-1)`) : le serveur X
  est aussi un enfant du processus, suivi par Smithay.

## Terminé quand

- Essai en imbriqué : trois terminaux ouverts par `Mod+Entrée` puis fermés ;
  `ps --ppid <pid du compositeur>` ne montre aucun `<defunct>` après le délai
  du minuteur.
- Le dock est toujours relancé par `coquille-dock-watch` quand on le tue, et
  s'arrête toujours avec le compositeur.

## Liens

- Aucun.
