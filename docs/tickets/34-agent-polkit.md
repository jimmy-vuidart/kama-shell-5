# 34 — Agent polkit : à essayer depuis l'écran de connexion

- **Priorité :** haute
- **Nature :** vérification
- **Zone :** `crates/polkit` (`coquille-polkit`), `scripts/coquille-session`
- **Se vérifie :** dans la vraie session, ouverte depuis l'écran de connexion

## Fait

`coquille-polkit` (détail dans `crates/polkit/README.md`) : agent
d'authentification sur le bus système, enregistré pour la session de
connexion, demande de mot de passe dans le style du shell, une demande à la
fois, sur l'écran latéral pendant un jeu. `coquille-session` le fait lancer
par le compositeur ; `scripts/install.sh` l'installe.

Vérifié le 26 octobre 2026 dans la session coquille, avec l'agent lancé à la
main : `setsid pkexec true` sans terminal aboutit avec le bon mot de passe,
échoue proprement avec un mauvais, « Annuler » rend la main (`Request
dismissed`) ; deux demandes ensemble s'affichent l'une après l'autre.

## Reste à vérifier

- Après `scripts/install.sh` et une nouvelle connexion : `polkit.log`
  dit `registered with polkit`, sans agent lancé à la main.
- Une action d'udisks (monter un disque interne depuis le gestionnaire de
  fichiers) aboutit.
- Une demande pendant un jeu ancré au centre s'ouvre sur l'écran latéral, et
  se remplit au clavier sans quitter le jeu (vu en imbriqué seulement).

**Prudence** : chaque mauvais mot de passe compte pour `pam_faillock`, trois
de suite verrouillent le compte dix minutes (`faillock --user jimmy`).

## Liens

- Étape 2 de la phase 5 (`docs/phase-5.md`), faite.
