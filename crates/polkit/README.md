# coquille-polkit

L'agent polkit de la session coquille : quand une appli a besoin d'un
administrateur (mise à jour, disque interne à monter, heure à changer,
`pkexec` lancé sans terminal), polkit demande à l'agent de la session, qui
affiche une demande de mot de passe.

- **D-Bus système** : il sert `org.freedesktop.PolicyKit1.AuthenticationAgent`
  (`/org/coquille/PolkitAgent`) et s'enregistre auprès de l'autorité de
  polkit pour la session de connexion (`XDG_SESSION_ID`, sinon celle que
  logind lui connaît). Il ne répond qu'à polkit (propriétaire du nom
  `org.freedesktop.PolicyKit1`), et s'enregistre de nouveau si polkit
  redémarre.
- **Il ne vérifie jamais le mot de passe lui-même** : c'est l'assistant de
  polkit, `polkit-agent-helper-1`, qui fait tourner PAM. Avec polkit 126 et
  plus, systemd le lance pour chaque connexion à
  `/run/polkit/agent-helper.socket` ; sans la socket, l'agent lance
  l'assistant setuid. Le protocole (identifiant, cookie, puis questions de
  PAM échappées par `g_strescape`) est décrit en tête de `src/helper.rs`.
- **L'assistant ne démarre qu'à la validation** : une conversation coupée
  (fenêtre annulée, autre compte choisi, appli qui abandonne) compte comme
  un échec pour `pam_faillock`, qui verrouille le compte après trois. Ouvrir
  la demande puis l'annuler ne coûte donc rien.
- **Une demande à la fois** : les suivantes attendent (« Une autre demande
  attend son tour »).
- **La fenêtre** : un panneau layer-shell `Overlay` (`coquille-polkit`), qui
  prend le clavier tant qu'il est là. Le compositeur le pose sur l'écran du
  pointeur ; pendant un jeu, sur l'écran latéral utilisé en dernier (règle
  7), par-dessus le jeu seulement si des jeux tiennent tous les écrans. Le
  mot de passe se tape sans sortir du jeu. `Entrée` valide, `Échap` annule.
- Le mot de passe ne va ni dans le journal ni dans le canal du shell, et est
  effacé de la mémoire de l'agent après usage (`zeroize`).

`coquille-session` le fait lancer par le compositeur (session ouverte depuis
l'écran de connexion seulement) ; il s'arrête avec lui. Son journal :
`~/.local/state/coquille/polkit.log`.

## L'essayer

Sans polkit, une demande inventée (le bon mot de passe est `coquille`) :

```sh
cargo build -p coquille -p coquille-polkit
target/debug/coquille --role center --spawn "$PWD/target/debug/coquille-polkit --demo"
```

Pour de vrai, dans une session coquille où aucun agent ne tourne :

```sh
target/debug/coquille-polkit &
setsid -w pkexec true </dev/null   # sans terminal : pkexec n'a pas son agent textuel
```

À côté d'un autre agent (Plasma), `--process PID` ne sert que les demandes
d'un processus, comme `pkttyagent --process`.

**Attention aux essais** : chaque mauvais mot de passe compte pour
`pam_faillock` (`faillock --user $USER` pour voir le compte). Trois de suite
verrouillent le compte, `sudo` compris, pour dix minutes.

## Ce qui n'est pas fait

- les messages de PAM qui précèdent la question (lecteur d'empreintes) ne
  s'affichent qu'une fois le mot de passe validé, puisque l'assistant ne
  démarre qu'alors : l'empreinte n'est pas prise en charge ;
- pas d'icône de l'action (polkit en donne rarement une) ni de détails
  dépliables ;
- un agent qui tombe n'est pas relancé (à la différence du dock).
