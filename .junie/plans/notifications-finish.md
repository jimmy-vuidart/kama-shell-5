---
sessionId: session-261018-155414-p6tk
---

# État actuel

## Plan retrouvé : système de notifications

Le plan `.junie/plans/notifications-system.md` a été trouvé. Il prévoyait 4 étapes :

### ✅ Step 1 — Serveur FDO et modèle de file (TERMINÉ)
- `crates/dock/src/model/notify.rs` : 986 lignes, `NotifyState` complet avec `ingest`, `close`, `hide_toasts`, `effective_dnd`, `toast_bar`, `expire_due`, etc.
- `crates/dock/src/backend/notify.rs` : 352 lignes, serveur D-Bus `org.freedesktop.Notifications` avec zbus, subscription iced, gestion des icônes/image-data
- Branché dans `backend/mod.rs` et `Dock::subscription`
- **25 tests unitaires** passent tous

### ✅ Step 2 — Bulles Overlay en fin de dock (TERMINÉ)
- `crates/dock/src/ui/toasts.rs` : 210 lignes, cartes toast avec icône, résumé, corps, actions, bouton fermer
- `app.rs` : `ToastSurface`, `open_toasts()`, `sync_toasts()`, `dismiss_toast()`, `invoke_toast()`, `activate_notice()`
- Géométrie pure : `toast_geometry()`, `ToastSurfacePlan`, `toast_surface_plan()` dans le modèle
- Ancrage layer-shell Overlay : `Left|Bottom` / `Right|Bottom` / `Bottom|Right` selon l'orientation
- NPD effectif : `do_not_disturb` (manuel) OU `any(screen.anchored)` (auto pendant un jeu)

### ✓ Step 3 — Centre de contrôle et NPD (TERMINÉ)
- **Fait** : La bascule `Toggle::DoNotDisturb` est branchée sur `NotifyState.user_dnd` dans `sync_toasts()`
- **Fait** : Historique de session affiché dans le centre de contrôle (`drawer.rs` : `notification_history`, `history_item`, `relative_time`)
- **Fait** : Bouton « Tout effacer » (`Message::ClearHistory`)
- **Fait** : `Message::HistoryDismiss(id)` pour supprimer une notice individuelle
- **Fait** : `Notice.received` timestamp ajouté, rempli par le backend
- **Fait** : Section masquée si l'historique est vide, tronquée à 20 entrées

### ✓ Step 4 — Docs et phase 5 (TERMINÉ)
- **Fait** : `README.md` : « Ne pas déranger n'a pas d'effet » retiré, section « Notifications » ajoutée
- **Fait** : `phase-5.md` : case `[x] 1. Notifications` cochée, description mise à jour


# Changements proposés

### Step 3 — Historique dans le centre de contrôle

**Fichier à modifier** : `crates/dock/src/ui/drawer.rs`

Ajouter une section « Notifications » dans le centre de contrôle :
- Liste déroulante des notices de `dock.notify.history` (plus récentes en haut, tronquée à ~20 lignes visibles)
- Chaque ligne : résumé tronqué, nom de l'appli, horodatage relatif
- Clic sur une ligne → active la fenêtre de l'appli
- Bouton « Tout effacer » vide l'historique
- Section masquée si l'historique est vide

**Fichier à modifier** : `crates/dock/src/app.rs`
- Ajouter `Message::ClearHistory` et `Message::HistoryDismiss(id)`
- Gérer dans `update()` : `notify.clear()` et `notify.forget(id)`

### Step 4 — Mise à jour de la documentation

**`crates/dock/README.md`** :
- Retirer « Ne pas déranger n'a pas d'effet » des limites
- Ajouter une section « Notifications » décrivant le serveur FDO, les bulles overlay, l'historique et le NPD

**`docs/phase-5.md`** :
- Cocher la case `[x] 1. Notifications`
- Mettre à jour la description pour refléter l'implémentation réelle
