# Prompt Claude Code — dock + tiroir (une seule réalisation)

Lis d'abord `CLAUDE.md` en entier et `docs/tasks/PHASE_4.md`, puis ouvre ces maquettes dans
`docs/maquettes/dock/` :

- `07-dock.png` : le dock au repos, au survol, au clic droit ;
- `04-soiree-raid.png` : le dock en situation, à l'échelle réelle ;
- `08-tiroir.png` : le tiroir ouvert, avec son centre de contrôle et le mixeur par application.

`07` montre l'écran gauche **agrandi ×1,5** : les dimensions réelles sont celles de la maquette divisées
par 1,5 (les mesures clés sont données plus bas).

`04` montre encore une barre sur le bord droit de l'écran droit : **elle est abandonnée**, ne pas en
tenir compte. Les maquettes d'origine (`docs/maquettes/shell/`) ne font plus référence pour les barres
ni pour le lanceur.

## Objectif

Réaliser **en un seul livrable** tout ce qui vit sur le bord gauche de l'écran gauche :

1. **le dock** (barre fine) ;
2. **le tiroir d'applications** qu'il ouvre (recherche, Épinglées, Applications paginées, Jeux) ;
3. **le centre de contrôle** en pied de tiroir (bascules, volume, luminosité, session) ;
4. **le mixeur par application** ouvert depuis le centre de contrôle.

Ce sont des vues d'**un même programme** : crate `dock`, binaire `coquille-dock`, qui gère plusieurs
surfaces layer-shell sur l'écran gauche (la barre, le tiroir, les menus et le mixeur) et partage un seul
état. Il n'y a rien d'autre à construire sur les autres écrans.

**C'est le contenu de la phase 4** : le dock est le nouveau lanceur. Le compositeur coquille existe
(phases 0 à 3 terminées, session lancée depuis GDM sur mes trois écrans) ; le dock tourne **dans
coquille**, pas dans une autre session.

## Situation de départ (à vérifier, pas à supposer)

- Workspace : crates `model` (logique pure) et `compositor`. Les crates `ipc` et `dock` sont à créer.
- **Le dock ne peut tourner nulle part aujourd'hui** : coquille n'implémente pas encore
  `zwlr_layer_shell_v1`, et ma session GNOME ne le propose pas. Il faut donc commencer par le
  compositeur (voir « Prérequis »).
- Le modèle sait déjà réserver une zone par écran (`OutputInfo::reserved`, évitée par les emplacements
  et les fenêtres flottantes) ; le compositeur la laisse à zéro.
- Le modèle connaît les fenêtres, leur écran, leur emplacement, leur état (active, réduite, ancrée) et
  la mémoire par appli. Rien de cela n'est exposé hors du compositeur.
- Les rôles des écrans viennent de `[outputs]` dans `config.toml` (chez moi : gauche `HDMI-A-1`,
  centre `DP-2`, droite `DP-1`).
- Le thème est choisi au démarrage du compositeur (clé `theme`, `"light"` ou `"dark"`), l'accent est
  fixe, et il n'y a ni notifications ni éclairage nocturne.

## Prérequis côté compositeur

À faire **avant** ou **avec** le dock, parce qu'il en dépend. Chacun suit les règles de `CLAUDE.md`
(règles de placement dans `model` avec leurs tests, pas d'`unsafe`, API de Smithay vérifiée dans la
version épinglée).

1. **Protocole layer-shell.** Plusieurs surfaces par client, calques, ancrages, région d'entrée,
   popups rattachés à une surface (menu contextuel, mixeur), et focus clavier **à la demande** (le dock
   ne le prend jamais, le tiroir le prend pour la recherche puis le rend à la fenêtre qui l'avait).
   Une surface layer-shell n'est pas une fenêtre : ni barre de titre, ni emplacement, ni mémoire.
2. **Zone exclusive.** Celle du dock alimente `OutputInfo::reserved` : les emplacements de l'écran
   gauche se resserrent quand il apparaît et se rétablissent quand il disparaît.
3. **Mode jeu.** Aucun panneau sur un écran ancré par un jeu : si l'écran gauche est ancré, le dock et
   le tiroir se retirent, et reviennent quand il est libéré.
4. **Canal IPC.** Crate `ipc` (JSON sur socket Unix) et son **implémentation dans le compositeur**,
   dans les deux sens :
   - état poussé : écrans (rôle, disposition, ancré ou non), fenêtres (appli, titre, écran,
     emplacement, active, réduite), mémoire par appli, thème ;
   - commandes : activer ou restaurer une fenêtre, la fermer, ouvrir une appli sur un écran donné,
     oublier la place d'une appli, basculer le tiroir, changer de thème, quitter la session.
   Une commande inconnue ou mal formée ne fait jamais tomber le compositeur.
5. **Règles nouvelles dans le modèle**, avec leurs tests :
   - « ouvrir sur un écran » : la prochaine fenêtre d'une appli donnée s'ouvre sur l'écran demandé,
     une seule fois, sans changer sa mémoire ;
   - oublier la place d'une appli ;
   - les jeux s'ouvrent au centre.
6. **Thème modifiable en cours de session**, et accent lu dans la configuration.
7. **Fichier de configuration commun.** Le compositeur refuse aujourd'hui toute clé qu'il ne connaît
   pas : il doit accepter les sections du dock (`[dock]`, `[drawer]`) et la clé `accent`, sans quoi
   toute ma configuration retombe sur les valeurs par défaut.
8. **Session.** `scripts/install.sh` installe `coquille-dock`, `coquille-session` le lance avec le
   compositeur et le relance s'il tombe, journal dans `~/.local/state/coquille/`. À faire en dernier,
   et seulement quand je le demande.
9. **Outil de test.** `--debug-input` ne sait rejouer ni clic droit, ni clic milieu, ni molette : à
   ajouter, le dock en a besoin pour être vérifié en mode imbriqué.

Lié, sans être bloquant : la lecture des fichiers `.desktop` faite pour le tiroir donnerait aussi les
vrais noms et icônes aux barres de titre, qui affichent aujourd'hui un carré de couleur et un nom déduit
de l'identifiant de l'appli. À me proposer une fois le catalogue en place.

## Avant d'écrire du code

1. Vérifie les services présents : PipeWire (et `wpctl`), NetworkManager, BlueZ, logind, `ddcutil` /
   accès `i2c` pour la luminosité, Steam (`~/.steam` ou `~/.local/share/Steam`). Si un outil manque,
   montre la commande d'installation sans l'exécuter.
2. Regarde ce que Smithay offre pour layer-shell dans la version épinglée, et comment anvil et niri
   s'en servent.
3. Choisis la bibliothèque d'interface. Piste prioritaire : **iced** avec un crate layer-shell
   (`iced_layershell` ou équivalent maintenu) ; alternative : `smithay-client-toolkit` + le rendu déjà
   utilisé pour les barres de titre (tiny-skia et cosmic-text). Vérifie les versions actuelles et leur
   documentation, ne te fie pas à ta mémoire. Critères : plusieurs surfaces layer-shell dans un même
   processus, transparence, animations, popups, région d'entrée réglable, saisie clavier dans le
   tiroir (recherche).
4. Présente-moi un plan court (bibliothèque et pourquoi, découpage en modules, backends, tests, étapes)
   et **attends ma validation**.

## Configuration (`~/.config/coquille/config.toml`)

Le fichier est partagé avec le compositeur ; les réglages généraux précèdent les sections.

```toml
theme = "dark"          # "dark" | "light" ; clé déjà lue par le compositeur
accent = "#7aa7ff"      # nouvelle clé, lue par le compositeur et par le dock

[dock]
favorites = ["firefox", "discord", "kitty", "code", "obsidian"]

[drawer]
pinned = ["firefox", "discord", "kitty", "code", "obsidian", "mpv", "org.gnome.Nautilus"]
```

Le dock se place sur l'écran qui a le rôle **gauche** pour le compositeur (section `[outputs]`), appris
par l'IPC : pas de connecteur à répéter dans `[dock]`.

Les volumes par appli sont mémorisés dans `~/.local/state/coquille/`.

## Apparence (pixels logiques réels)

### Dock

- Largeur **44**, à **8** du bord gauche, centré verticalement, rayon **14**.
  Verre liquide (`palette.glass`) avec liseré clair (`palette.rim`) et ombre douce.
  Le flou d'arrière-plan est fourni par le compositeur.
- Emplacement d'icône **36 × 36** (rayon 10), icône **24**. Indicateurs à gauche : barre d'accent **3 × 12**
  = ouverte ici ; point gris **3 × 4** = ouverte sur un autre écran. Badge non-lus rouge `#e5484d`.
- Heure en bas sur deux lignes, gras. Déploiement au survol : largeur ~**200**, noms et sous-titres,
  titres « FAVORIS » / « SUR CET ÉCRAN », heure complète et date.
- Menu contextuel ~**240** de large en `palette.glass_strong` avec liseré `palette.rim` : « OUVRIR SUR » Gauche / Centre / Droite, Nouvelle fenêtre,
  Retirer des favoris (ou Épingler), Oublier sa place, Fermer la fenêtre (rouge, si ouverte).

### Tiroir (`08-tiroir.png`)

- Glisse depuis le bord gauche, **sur toute la hauteur** de l'écran gauche, largeur ~**640**,
  coins droits arrondis (~**18**), le reste de l'écran voilé (`palette.scrim`). Le dock reste visible, intégré au tiroir,
  bouton du lanceur en accent. Matière en verre liquide (`palette.glass`) avec liseré (`palette.rim`), flouté par le compositeur.
- De haut en bas : **recherche** ; **Épinglées** (une rangée, étiquette d'emplacement sous chaque icône) ;
  **Applications** (grille 6 × 3, « 47 · de A à Z · page 1 sur 3 », flèches et numéros de page) ;
  **Jeux** (une rangée de jaquettes, lien « Toute la ludothèque ») ; **pas de section « récents »**.
- Pied = **centre de contrôle** dans un bloc de carte (`palette.card`) : cinq bascules (Réseau, Bluetooth, Ne pas déranger,
  Éclairage nocturne, Thème sombre ; tuiles éteintes en `palette.glass_strong` avec liseré `palette.rim`, activée = fond d'accent), curseurs Volume (bouton « Par appli »)
  et Luminosité (« 3 écrans »), puis utilisateur à gauche et Paramètres, Se déconnecter, Redémarrer,
  Éteindre (rouge) à droite.
- **Mixeur** : panneau ~**290** de large à droite du tiroir, pointant vers le bouton « Par appli » :
  fond en `palette.glass_strong` avec liseré `palette.rim`, bloc intérieur en `palette.card`, sortie audio, volume général, une ligne par appli (icône, nom, état, curseur, niveau en direct en vert,
  bouton couper le son rouge quand coupé), note sur la mémorisation des volumes.

Thèmes clair et sombre (anthracite translucide), accent depuis la config (pervenche par défaut), textes en français, police Figtree (repli sans-serif).

## Comportement

- **Dock** : zone exclusive de **60 px** à gauche ; le déploiement au survol passe par-dessus les fenêtres ;
  la région d'entrée se limite à la partie visible ; le dock ne prend jamais le focus clavier.
  Clic gauche : lancer ou activer (plusieurs fenêtres : la plus récente, puis la suivante ; une fenêtre
  réduite est restaurée) ; clic milieu : nouvelle fenêtre ; clic droit : menu ; glisser dans la barre :
  réordonner les favoris.
- **Tiroir** : s'ouvre au clic sur le lanceur, et par `ToggleLauncher` sur l'IPC pour un raccourci du
  compositeur. Il prend le focus clavier pour la recherche : la saisie filtre aussitôt, flèches pour
  naviguer, `Entrée` ouvre, `Échap` ferme. Clic en dehors : fermeture. Molette sur la grille ou glisser
  horizontal : page suivante / précédente.
  Les jeux s'ouvrent toujours au centre ; les applis à l'emplacement indiqué par leur étiquette.
  **Glisser une icône vers un écran** pour l'y ouvrir : prévois l'API, la réalisation est une étape
  optionnelle en fin de liste.
- **Centre de contrôle** : bascules et curseurs agissent sur les backends réels quand ils sont actifs.
  Molette sur un curseur pour le régler.
- **Mixeur** : ouvert / fermé par « Par appli » ; liste mise à jour en direct quand une appli commence
  ou arrête de jouer du son ; chaque appli retrouve son volume à sa prochaine ouverture.
- **Sécurité** : Redémarrer, Éteindre et Se déconnecter demandent une **confirmation** et restent
  **simulés** (journalisés seulement) tant que je n'ai pas activé `--backends session`.
  Pendant le développement, aucune action ne doit pouvoir couper ma session par accident.

### Décisions

- **Quel raccourci ouvre le tiroir.** `Super` seul libère déjà le pointeur d'un jeu (phase 3). Enlever ce raccourci pour Super+Escape, et utiliser Super pour le tiroir.
- **Bouton « ajouter un onglet »** des barres de titre : supprimer cette fonctionnalité. Avec le dock ça n'a plus de sens
- **Éclairage nocturne et ne pas déranger** : bascules sans effet pour l'instant

## Ce que le dock reprend, et ce qui reste sans support

Depuis l'abandon des barres latérales, plusieurs fonctions n'avaient plus d'endroit où vivre
(tableau de `docs/tasks/PHASE_4.md`) :

| Fonction | Avec le dock |
|---|---|
| Lancer une appli | tiroir, favoris |
| Retrouver une fenêtre réduite | clic sur son icône dans le dock |
| Voir les fenêtres d'un écran | écran gauche seulement (« SUR CET ÉCRAN ») |
| Heure, réseau, son | dock et centre de contrôle |
| Bascule « ne pas déranger » | centre de contrôle, sans effet tant qu'il n'y a pas de notifications |
| Changer la disposition d'un écran | **toujours rien à la souris** (`Super+1` à `5`) |
| « Tout envoyer vers l'écran… » | **toujours rien** |
| Afficher les notifications | **toujours rien** (concept conservé) |
| Enregistrer et restaurer un instantané | **toujours rien** (concept conservé) |

Ne rien inventer pour les lignes « toujours rien » : me les signaler si une place naturelle apparaît.

## Tests et qualité

- Tests unitaires des trois modèles : ordre et indicateurs du dock, menu selon l'état, survol ;
  tri et pagination (47 applis → 3 pages, dernière page partielle), recherche avec accents,
  emplacement d'ouverture des épinglées ; agrégation des flux audio par appli et mémoire des volumes.
- Tests du protocole `ipc` (aller-retour JSON, commande inconnue) et des règles ajoutées au modèle.
- `scripts/ci.sh` vert (`cargo fmt`, `cargo clippy -- -D warnings`, tests) avant chaque commit.
- Vérification en mode imbriqué, par entrées rejouées et captures :
  `cargo run -p coquille -- --three-screens --spawn "coquille-dock --mock" --screenshot …`.
- `crates/dock/README.md` : lancer en mode simulé ou avec les backends réels, options, limites.

## Étapes (arrête-toi à la fin de chacune pour que je teste)

1. Plan validé, puis **layer-shell dans le compositeur** (prérequis 1 à 3) et l'outil de test complété
   (prérequis 9). Preuve : un panneau d'essai qui réserve sa zone sur l'écran gauche.
2. Les trois modèles + tests, et les messages de la crate `ipc`.
3. **Dock** en rendu statique avec `--mock` dans coquille imbriqué : barre, icônes, indicateurs, badge,
   heure, thèmes. Montre-moi la commande pour le lancer.
4. **IPC côté compositeur** et règles du modèle (prérequis 4 à 7) : le dock montre les vraies fenêtres.
5. **Dock interactif** : survol déployé, clics, menu contextuel (ouvrir sur, oublier sa place, fermer),
   réordonnancement.
6. **Tiroir** (maquette `08` fournie) : ouverture / fermeture, recherche, épinglées, applications
   paginées, jeux (Steam réel).
7. **Centre de contrôle et mixeur** avec les backends réels disponibles (PipeWire d'abord, puis
   NetworkManager, BlueZ, luminosité), session toujours simulée.
8. **Intégration à la session** (prérequis 8), quand je le demande.
9. Optionnel, à me proposer : non-lus réels via l'API D-Bus `com.canonical.Unity.LauncherEntry` ;
   glisser une icône vers un écran.

Ne touche ni à GDM ni à ma session par défaut, n'installe aucun paquet sans me montrer la commande, ne
touche pas aux permissions `i2c` sans me l'expliquer d'abord, n'exécute pas `scripts/install.sh` à ma
place, et ne fais rien démarrer avec la session avant l'étape 8.
