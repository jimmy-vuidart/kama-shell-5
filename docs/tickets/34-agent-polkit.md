# 34 — Pas d'agent polkit

- **Priorité :** haute
- **Nature :** fonction manquante
- **Zone :** `scripts/coquille-session`, nouveau `crates/polkit` (binaire `coquille-polkit`), `scripts/install.sh`
- **Se vérifie :** sur un bus D-Bus système, donc dans la vraie session (ou une machine virtuelle)
- **Certitude :** absence lue dans le code ; le protocole de `polkit-agent-helper-1` est à vérifier sur la version de polkit installée

## Constat

Aucun programme de la session coquille ne s'enregistre auprès de polkit
comme agent d'authentification. Sous Plasma, c'est `polkit-kde-agent` ; son
entrée de démarrage automatique est réservée à KDE (`OnlyShowIn=KDE`), et
`coquille-session` ne lance rien de tel.

## Conséquence

Toute action qui demande le mot de passe administrateur depuis une appli
graphique échoue en silence, ou avec une erreur « non autorisé » :
mises à jour graphiques, GParted, monter un disque interne depuis le
gestionnaire de fichiers, changer l'heure, gérer une imprimante. `pkexec`
dans un terminal marche encore (il a son propre agent textuel), ce qui
masque le manque.

## À décider avec Jimmy

- En attendant `coquille-polkit`, lancer `polkit-gnome` (paquet
  `polkit-gnome`, à installer : `sudo pacman -S polkit-gnome`) ?
- Pendant un jeu ancré : la demande s'affiche-t-elle par-dessus le jeu
  (comme le tiroir quand des jeux tiennent tous les écrans), ou sur un écran
  latéral ?

## Piste

1. **Provisoire** : `coquille-session` lance
   `/usr/lib/polkit-gnome/polkit-gnome-authentication-agent-1` quand il
   existe et que la session vient de l'écran de connexion (comme le
   trousseau, arrêté avec le compositeur par `--until` ou équivalent).
2. **`coquille-polkit`**, programme iced dans le style du shell :
   - D-Bus système (`zbus`, déjà utilisé par le dock et le trousseau) :
     servir `org.freedesktop.PolicyKit1.AuthenticationAgent`
     (`BeginAuthentication`, `CancelAuthentication`) et s'enregistrer par
     `RegisterAuthenticationAgent` de
     `org.freedesktop.PolicyKit1.Authority`, pour la session de connexion
     (`unix-session`, identifiant de logind) ;
   - authentifier par l'assistant setuid de polkit,
     `/usr/lib/polkit-1/polkit-agent-helper-1` : nom de l'utilisateur en
     argument, cookie sur l'entrée, conversation PAM par lignes
     (`PAM_PROMPT_ECHO_OFF`, `PAM_TEXT_INFO`, `PAM_ERROR_MSG`, puis
     `SUCCESS` ou `FAILURE`). Les versions récentes de polkit passent par une
     socket activée par systemd (`polkit-agent-helper.socket`) : vérifier
     laquelle est installée ;
   - une fenêtre layer-shell `Overlay`, clavier `Exclusive`, au milieu de
     l'écran du pointeur : icône et message de l'action, choix de l'identité
     s'il y a plusieurs administrateurs, champ de mot de passe, « Annuler » et
     « Authentifier », erreur affichée sans fermer après un mauvais mot de
     passe ;
   - le mot de passe ne va jamais dans le journal ni dans le canal du shell.
3. Installation : `scripts/install.sh` installe le binaire, `coquille-session`
   le lance à la place de l'agent provisoire.

## Terminé quand

- Dans la vraie session, avec Jimmy : `pkexec true` lancé depuis le dock (pas
  depuis un terminal) affiche la demande, aboutit avec le bon mot de passe,
  échoue proprement avec un mauvais, et « Annuler » rend la main à l'appli.
- Une action d'udisks (monter un disque interne depuis le gestionnaire de
  fichiers) aboutit.
- Une seule demande à la fois, même si deux applis demandent ensemble.

## Liens

- Lancé comme le trousseau ; pourra passer par la cible du ticket 35.
- Étape 2 de la phase 5 (`docs/phase-5.md`).
