# coquille-keyring

Le trousseau de la session coquille : il sert `org.freedesktop.secrets` (le
Secret Service de freedesktop, que libsecret, Chrome, les applis Electron, `gh`
ou JetBrains utilisent) et le portail `org.freedesktop.impl.portal.Secret`
(un secret par appli Flatpak).

Il est toujours ouvert et ne demande jamais rien. Ses secrets sont dans
`~/.local/state/coquille/keyring.bin`, chiffrés en AES-256-GCM ; la clé est
scellée dans `keyring.key` par `systemd-creds --user` (TPM2 et clé de la
machine). Une clé perdue ou fausse le fait refuser de démarrer, sans toucher
au fichier.

`coquille-session` le lance avant le compositeur (session ouverte depuis
l'écran de connexion seulement) et l'arrête avec lui. Son journal :
`~/.local/state/coquille/keyring.log`.

## L'essayer sans toucher au trousseau de la session

Sur un bus D-Bus à part, avec une clé brute plutôt que scellée :

```sh
cargo build -p coquille-keyring
dbus-run-session -- sh -c '
  target/debug/coquille-keyring --state-dir /tmp/essai --key-file /tmp/essai.key &
  sleep 0.5
  printf secret | secret-tool store --label=Essai service essai
  secret-tool lookup service essai'
```

Sans `--key-file`, la clé est scellée par le TPM dans le dossier donné.

Options :

- `--state-dir DOSSIER` : où garder le trousseau (défaut `~/.local/state/coquille`) ;
- `--key-file FICHIER` : une clé brute de 32 octets, créée si elle manque (essais seulement) ;
- `--until PID` : s'arrêter quand ce processus se termine.

`RUST_LOG=coquille_keyring=debug` montre les sessions ouvertes et les éléments
enregistrés (jamais les secrets).

## Ce qui n'est pas fait

Pas de verrouillage ni d'invite (choix voulu), pas de reprise de gnome-keyring
ou de KWallet, pas d'agent SSH, pas des interfaces propres à gnome-keyring
(Seahorse).
