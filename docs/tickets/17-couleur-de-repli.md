# 17 — La couleur de repli d'une appli diffère entre le dock et la capsule

- **Priorité :** faible
- **Nature :** bug (apparence)
- **Zone :** `crates/compositor/src/decoration/naming.rs`, `crates/dock/src/ui/icons.rs`
- **Se vérifie :** en imbriqué
- **Certitude :** lu dans le code

## Constat

Une appli sans icône reçoit un carré de couleur, toujours le même pour elle.
Cette couleur est calculée deux fois, de deux façons :

| | Compositeur | Dock |
|---|---|---|
| Où | `naming.rs:17`, `badge_color` | `ui/icons.rs:99`, `tile_color` |
| Palette | huit couleurs, « celles des maquettes » | huit couleurs, pas les mêmes ni dans le même ordre |
| Hachage | FNV-1a | somme multipliée par 31 |
| Clé | l'`app_id` de la fenêtre | `identity.key` |

## Conséquence

La même appli a une couleur dans le dock et une autre sur sa capsule et sa
barre de titre.

## Piste

Une seule fonction, dans une crate que les deux programmes utilisent déjà et
qui ne dépend pas d'iced : `coquille-apps` aujourd'hui, ou la crate de jetons
du ticket 21 si elle existe. Elle rend trois octets ; chaque programme les
convertit dans son type de couleur.

Garder la palette du compositeur, qui suit les maquettes, et vérifier que les
deux côtés passent bien la même clé pour une même appli.

## Terminé quand

- Un test vérifie que le dock et le compositeur donnent la même couleur à une
  même appli.
- Essai en imbriqué avec une appli sans icône : même couleur dans le dock,
  sur la capsule et dans la barre de titre.

## Liens

- Se fait bien avec : 21.
