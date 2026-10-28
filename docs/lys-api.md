# API lys-en-france (`/lys`)

Base : `https://chetaku.chetana.fr`. Toutes les routes exigent `Authorization: Bearer <jeton Logto>` d'audience `https://lys-en-france.chetana.fr/api` et un `sub` de la liste blanche. JSON UTF-8. CORS : origines `https://lys-en-france.chetana.fr` et `http://localhost:5173`. Corps limité à 16 Kio (sauf l'envoi de document, 12 Mio).

## Erreurs (toutes les routes)

Format unique : `{ "error": "<message>" }`.

| Code | Cas |
|---|---|
| 400 | identifiant de document ou de rendez-vous qui n'est pas un UUID canonique ; `starts_at` hors 2026-01-01 à 2030-12-31 ; lieu > 200 ou note de rendez-vous > 500 caractères ; km-reviewed sur une tâche sans texte khmer ; fichier vide ; JSON mal formé ; valeur refusée (statut hors liste, note > 2000 caractères ou avec caractère de contrôle autre que `\n`, corps PATCH vide) |
| 401 | jeton absent, invalide, expiré, mauvaise audience |
| 403 | jeton valide, `sub` hors liste blanche (`{"error":"Forbidden"}`) : le front affiche la page « changer de compte » |
| 404 | slug inconnu (`{"error":"Not found"}`) |
| 413 | corps > 16 Kio (document : > 12 Mio) |
| 415 | document dont le type réel n'est pas PDF, JPEG, PNG ou HEIC/HEIF |
| 422 | JSON valide mais champ inconnu, manquant ou de mauvais type (ex. date illisible) |
| 429 | limite par IP (120 req/min, corps sans `kind`) ; sur `POST /lys/chat` : `{"error":"…","kind":"daily_limit"}` |
| 502 | stockage des documents en erreur (`{"error":"stockage en erreur"}`, détail dans les logs seulement) |
| 503 | stockage des documents non configuré (`{"error":"stockage indisponible"}`) ; chat désactivé ou Cline en échec (`{"kind":"unavailable"}`) ; le reste de `/lys` fonctionne |
| 500 | erreur interne, message générique `Internal error` |

## GET /lys/me

`200 {"sub": "yjbv975dazhd"}`

## GET /lys/plan

Tout le plan en un appel (instantané cohérent). Aucun paramètre.

```json
{
  "totals": { "tasks_done": 0, "tasks_total": 12, "items_done": 1, "items_total": 44 },
  "next": {
    "slug": "vls-ts-anef",
    "category_slug": "sejour",
    "title_fr": "Valider le VLS-TS sur l'ANEF",
    "title_km": null,
    "status": "doing",
    "effective_due": "2026-11-09",
    "legal_deadline": null
  },
  "agenda": [
    {
      "type": "appointment", "date": "2026-11-09", "starts_at": "2026-11-09T13:30:00Z",
      "place": "Préfecture de Melun", "note": "Apporter les originaux", "appointment_id": "123e4567-e89b-42d3-a456-426614174001",
      "task_slug": "vls-ts-anef", "title_fr": "…", "title_km": null, "days_left": 13
    },
    {
      "type": "due", "date": "2026-11-20", "starts_at": null, "place": null, "note": null, "appointment_id": null,
      "task_slug": "cv", "title_fr": "…", "title_km": "…", "days_left": 24
    }
  ],
  "categories": [
    {
      "slug": "sejour", "position": 1, "title_fr": "Titre de séjour", "title_km": null,
      "tasks_done": 0, "tasks_total": 3, "items_done": 1, "items_total": 17,
      "tasks": [
        {
          "slug": "vls-ts-anef", "category_slug": "sejour", "position": 1,
          "title_fr": "…", "title_km": null, "help_fr": "…", "help_km": null,
          "status": "doing",
          "note": "",
          "due_date": "2026-11-09", "due_override": null, "legal_deadline": null,
          "effective_due": "2026-11-09",
          "done_at": null,
          "km_status": "none", "km_reviewed_by": null, "km_reviewed_at": null,
          "appointments": [
            {
              "id": "123e4567-e89b-42d3-a456-426614174001", "starts_at": "2026-11-09T13:30:00Z",
              "place": "Préfecture de Melun", "note": "Apporter les originaux",
              "created_by": "yjbv975dazhd", "created_at": "2026-10-27T18:02:11.123456Z"
            }
          ],
          "items_done": 1, "items_total": 8,
          "documents": [
            {
              "id": "123e4567-e89b-42d3-a456-426614174000", "item_slug": "vls-ts-anef.1",
              "filename": "Acte de naissance.pdf", "content_type": "application/pdf",
              "size_bytes": 1048576, "uploaded_by": "yjbv975dazhd", "uploaded_at": "2026-11-03T18:02:11.123456Z"
            }
          ],
          "items": [
            {
              "slug": "vls-ts-anef.1", "kind": "step", "position": 1,
              "label_fr": "Créer le compte ANEF", "label_km": null,
              "done": true, "done_at": "2026-10-30T18:02:11.123456Z", "done_by": "yjbv975dazhd"
            }
          ]
        }
      ]
    }
  ]
}
```

Types :
- `status` : `"todo" | "doing" | "done" | "blocked"` (chaîne, jamais un enum fermé côté client : prévoir un repli).
- `kind` (item) : `"step" | "document"`. `km_status` : `"none" | "draft" | "reviewed"`.
- Dates `YYYY-MM-DD` ; horodatages RFC 3339 UTC ; champs absents = `null` (jamais omis).
- `title_km`, `help_km`, `label_km` : `null` tant que non traduits, le front retombe sur le français. `help_fr` peut aussi être `null`.
- `due_date` : échéance souhaitée (seed). `due_override` : échéance posée par l'utilisateur. `effective_due` = `due_override`, sinon `due_date`, sinon `null`. `legal_deadline` : échéance réglementaire (lecture seule en v1, saisie en base).
- `documents` (par tâche, aussi dans la `task` renvoyée par `PUT /lys/items/{slug}/done` et `PATCH /lys/tasks/{slug}`) : documents non supprimés, du plus ancien au plus récent ; `[]` sans document. `item_slug` vaut `null` pour un document rattaché à la tâche seule. Jamais d'URL ici : voir `GET /lys/documents/{id}/url`.
- Ordre : catégories par `position`, tâches par `position` dans leur catégorie, items par `position` dans leur tâche.
- Compteurs `tasks_done` = tâches au statut `done`. Pas de champ `priority` en v1 (absent du schéma) : l'ordre vient de l'échéance puis de la position.

### `agenda` (T3, champ ajouté ; les autres champs sont inchangés)

Liste unique, triée par `date` croissante : les rendez-vous à venir (non annulés, `starts_at` ≥ maintenant) et les échéances des tâches **non `done`** (une tâche `blocked` compte).

- `type` : `"appointment"` | `"due"` (échéance effective = `due_override`, sinon `due_date`) | `"legal"` (`legal_deadline`, distincte de `due` : une tâche peut produire les deux). Prévoir un repli pour un type inconnu.
- `date` : jour `YYYY-MM-DD` **en heure de Paris** (pour un rendez-vous, le jour local de `starts_at`). `days_left` : entier, jours calendaires entre aujourd'hui (Paris) et `date` ; **négatif** = en retard (les échéances dépassées restent dans la liste, en tête). `0` = aujourd'hui.
- À date égale : `legal`, puis `due`, puis rendez-vous par heure.
- `starts_at`, `place`, `note`, `appointment_id` : renseignés pour `appointment`, `null` sinon. `task_slug` + `title_fr` + `title_km` (`null` si non traduit : retomber sur le français) pour tous.
- `[]` si rien. Pas de pagination (quelques dizaines d'entrées au plus).

`appointments` (par tâche, aussi dans la `task` renvoyée par `PUT items/…/done`, `PATCH tasks/…`, et les routes ci-dessous) : rendez-vous non annulés de la tâche, passés compris, triés par `starts_at` ; `[]` sans rendez-vous. `km_reviewed_by` / `km_reviewed_at` : qui et quand a relu le khmer (`null` sinon).

### Règle de `next` (« à faire maintenant »)

Parmi les tâches ni `done` ni `blocked`, la première selon : (1) `effective_due` croissante, sans échéance en dernier, (2) `position` de la catégorie, (3) `position` de la tâche. `next` vaut `null` si tout est terminé ou bloqué. Les champs de `next` sont ceux ci-dessus (`slug`, `category_slug`, `title_fr`, `title_km`, `status`, `effective_due`, `legal_deadline`) ; le détail se lit dans `categories`.

## PUT /lys/items/{slug}/done

Corps : `{ "done": true }` (seul champ, obligatoire, booléen strict).

- Idempotent : répéter le même état ne change rien (`done_at` et `done_by` d'origine conservés). Cocher pose `done_at = maintenant`, `done_by = sub du jeton` ; décocher les remet à `null`.
- Réponse `200` : `{ "item": <item>, "task": <tâche complète avec items, comme dans /lys/plan> }`. `task` porte le statut recalculé et `items_done`/`items_total` : mettre l'interface à jour sans recharger le plan (les compteurs de catégorie et `next` se recalculent côté front ou par un nouveau GET).
- 404 si l'item n'existe pas, 422 si le corps est invalide.

### Règle de statut de la tâche

Appliquée dans la même transaction, tâche verrouillée (cochages concurrents sérialisés) :
1. Toutes les cases cochées → `done` (`done_at` de la tâche posé), même si la tâche était `blocked`.
2. Tâche `done` dont on décoche une case → `doing` s'il reste des cases cochées, sinon `todo`.
3. Tâche `todo` dont on coche une case → `doing`. Tâche `doing` dont on décoche la dernière case cochée → `todo`.
4. Sinon le statut est conservé : un `blocked` posé à la main ne bouge pas ; cocher une case d'une tâche mise à `done` à la main (cases non toutes cochées) ne la rouvre pas.

## PATCH /lys/tasks/{slug}

Corps JSON, tout champ inconnu → 422. Champs, tous optionnels, au moins un requis :

| Champ | Type | Règle |
|---|---|---|
| `status` | chaîne | `todo`, `doing`, `done`, `blocked`, sinon 400 |
| `note` | chaîne | ≤ 2000 caractères (pas octets), aucun caractère de contrôle sauf `\n` (donc ni `\t` ni `\r`), sinon 400. `""` efface la note. `null` est traité comme absent |
| `due_override` | `"YYYY-MM-DD"` ou `null` | `null` efface l'échéance modifiée ; date illisible → 422 |

Un champ absent n'est pas modifié. `status: "done"` pose `done_at` (conservé si déjà `done`) ; tout autre statut l'efface. Les cases ne sont pas touchées.

Réponse `200` : `{ "task": <tâche complète avec items> }`. 404 si le slug est inconnu (vérifié après la validation du corps : un corps invalide sur un slug inconnu donne 400/422).

## Rendez-vous

### POST /lys/tasks/{slug}/appointments

Corps (champ inconnu → 422) :

| Champ | Type | Règle |
|---|---|---|
| `starts_at` | chaîne | obligatoire, ISO 8601 **avec fuseau** (`2026-11-09T14:30:00+01:00` ou `…Z` ; en JS : `new Date(local).toISOString()`). Illisible ou sans fuseau → 422 ; hors 2026-01-01 à 2030-12-31 → 400. Stocké en UTC |
| `place` | chaîne ou `null` | ≤ 200 caractères, sans caractère de contrôle ; blanc = absent |
| `note` | chaîne ou `null` | ≤ 500 caractères, sans caractère de contrôle sauf `\n` ; blanc = absent |

`201 { "appointment": {id, starts_at, place, note, created_by, created_at}, "task": <tâche complète> }`. 404 si la tâche est inconnue (après validation du corps).

### DELETE /lys/appointments/{id}

`204` sans corps. Annulation douce (la ligne reste en base, `cancelled_at` posé), le rendez-vous disparaît du plan. Idempotent. `404` si l'`id` n'a jamais existé, `400` si ce n'est pas un UUID canonique.

## Relecture du khmer

### PUT /lys/tasks/{slug}/km-reviewed

Corps : `{ "reviewed": true }` (seul champ, booléen strict, sinon 422). `true` : la tâche passe `km_status = "reviewed"`, `km_reviewed_by` = `sub` du jeton, `km_reviewed_at` = maintenant ; ses cases au khmer `draft` passent aussi `reviewed`. `false` : retour à `"draft"` (qui/quand effacés, cases `reviewed` repassées `draft`). Idempotent. `400` si la tâche n'a aucun texte khmer (`km_status = "none"`), `404` si le slug est inconnu. Réponse `200 { "task": <tâche complète> }`. Un texte `reviewed` n'est jamais écrasé par un rejeu du seed khmer. Le front retire la pastille « traduction automatique » quand `km_status` de la tâche vaut `reviewed`.

## Documents

Stockage : bucket S3 privé, sans URL publique. Les 3 routes répondent `503 {"error":"stockage indisponible"}` si le stockage n'est pas configuré sur le serveur.

### POST /lys/tasks/{slug}/documents?item=&filename=

Envoi d'un fichier en **corps brut** (pas de multipart), le plus simple depuis le front :

```js
const r = await fetch(`${API}/lys/tasks/${task}/documents?item=${item}&filename=${encodeURIComponent(file.name)}`, {
  method: 'POST',
  headers: { Authorization: `Bearer ${token}` },
  body: file, // File/Blob tel quel ; pas de FormData
});
```

- `item` (optionnel) : slug d'un item DE CETTE TÂCHE, sinon 404. `filename` (optionnel) : nom d'affichage (encodé en URL) ; il est assaini (dernier segment de chemin, sans caractère de contrôle ni caractère réservé, 120 caractères max, « document » à défaut) et ne sert jamais à la clé de stockage.
- Le `Content-Type` et l'extension du client sont ignorés : le type est déterminé par les premiers octets du fichier. Acceptés : PDF, JPEG, PNG, HEIC/HEIF (`content_type` stocké : `application/pdf`, `image/jpeg`, `image/png`, `image/heic`, `image/heif`).
- Taille : 12 Mio maximum (`413` au-delà ; le navigateur peut voir une coupure de connexion au lieu du 413 pour un fichier très gros : contrôler `file.size` côté front avant l'envoi). Fichier vide : 400. Au plus 3 envois traités en même temps, les autres attendent.
- `201` : `{ "document": { "id", "item_slug", "filename", "content_type", "size_bytes", "uploaded_by", "uploaded_at" } }` (même forme que dans `documents` du plan).
- Erreurs : 404 tâche ou item inconnu, 415 type refusé, 413 trop gros.

### GET /lys/documents/{id}/url

`200 {"url": "https://…", "expires_in": 300}` : URL signée de lecture, valable 300 s, qui force le téléchargement (`Content-Disposition: attachment`, nom d'origine assaini). L'ouvrir par navigation (`<a href>` ou `window.location`), **jamais par `fetch`** (aucune CORS n'est posée sur le bucket). Ne pas la stocker ni la partager : redemander une URL à chaque ouverture. Réponse `Cache-Control: no-store`.

- `id` : UUID au format canonique (`xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`), sinon `400`.
- `404` si le document n'existe pas ou a été supprimé.

### DELETE /lys/documents/{id}

`204` sans corps. Suppression douce en base (le document disparaît du plan) et suppression de l'objet dans le bucket (qui est versionné : la version reste récupérable côté administration). Idempotent : refaire l'appel sur un document déjà supprimé renvoie `204`. `404` si l'`id` n'a jamais existé, `400` si ce n'est pas un UUID.

### Configuration serveur (variables d'environnement)

`LYS_S3_BUCKET`, `LYS_S3_ACCESS_KEY`, `LYS_S3_SECRET_KEY` (les trois requises, sinon 503) ; optionnelles : `LYS_S3_REGION` (défaut `fr-par`), `LYS_S3_ENDPOINT` (défaut `https://s3.<région>.scw.cloud`), `LYS_S3_PATH_STYLE=1` (S3 auto-hébergé).

## POST /lys/chat

Une question libre sur le plan du couple ; réponse en français et traduction khmère automatique. Même garde que le reste de `/lys` (401 sans jeton, 403 hors liste blanche), corps limité à 16 Kio, et le rate-limit global par IP s'applique.

Corps : `{ "question": string, "lang": "fr" | "km" }`. `question` : 1 à 500 **caractères** après `trim`, aucun caractère de contrôle (retour à la ligne compris). `lang` est validé mais ne change rien à la réponse : le serveur renvoie toujours les deux langues, le front choisit laquelle afficher.

`200` :

```json
{
  "answer_fr": "Commencez par valider le VLS-TS sur l'ANEF avant le mardi 2 février 2027.",
  "answer_km": "សូមចាប់ផ្តើម…",
  "machine_translated": true,
  "task_slugs": ["vls-ts-anef"],
  "remaining_today": 29,
  "id": "2026-10-28_0b9e1c52-3f0a-4c1e-9a55-6d2f7a1b8c34",
  "created_at": "2026-10-28T10:00:00Z"
}
```

- `answer_km` vaut `null` si la traduction a échoué ou a été refusée par les garde-fous (pas d'écriture khmère, écriture étrangère, latin collé au khmer, nombre ou date de la réponse française perdu ou modifié) : afficher le français seul. `machine_translated` vaut toujours `true` (pastille « traduction automatique »).
- `task_slugs` : tâches dont le slug ou le titre est cité dans la réponse, dans l'ordre du plan ; `[]` sinon.
- `remaining_today` : questions restantes aujourd'hui pour ce compte.
- `id`, `created_at` : identifiant public et horodatage (RFC 3339, UTC) de l'échange enregistré dans l'historique ; `null` tous les deux si l'enregistrement n'a pas eu lieu (stockage absent ou en panne). Un échec de stockage ne fait jamais échouer la requête.
- Les réponses sont courtes (6 phrases), fondées sur le plan et sur des dates calculées par le serveur ; si le plan ne dit pas, la réponse l'indique (« à confirmer sur le site officiel »). Hors sujet : refus poli.
- Confidentialité : seuls partent chez le fournisseur d'IA le plan (titres, aides, échéances, étapes et pièces), les dates clés et la question. Jamais les notes, documents, rendez-vous, ni le `sub`.

Erreurs, toutes au format `{ "error": "<message>", "kind": "<code>" }` :

| Code | `kind` | Cas |
|---|---|---|
| 400 | `invalid` | JSON mal formé, champ manquant ou de mauvais type, `lang` hors `fr`/`km`, question vide, > 500 caractères ou avec caractère de contrôle (ne consomme pas le quota) |
| 429 | `daily_limit` | 30 questions par compte et par jour, ou plafond global de 300 par jour atteint |
| 404 | `not_found` | (historique) échange introuvable |
| 503 | `unavailable` | chat désactivé sur le serveur, ou fournisseur d'IA en erreur / sans réponse en 25 s (la question n'est alors pas décomptée) |

Le corps d'erreur du fournisseur et la clé ne sont jamais renvoyés. Les 401/403 gardent leur format habituel (sans `kind`).

Limites :
- Compteurs **en mémoire** : remis à zéro au redémarrage du serveur et à minuit, heure de Paris. Ce sont des garde-fous de coût, pas un quota garanti ; plusieurs instances auraient chacune leurs compteurs.
- Deux appels successifs au fournisseur (réponse puis traduction), 25 s au plus chacun : prévoir un délai de 50 s côté front, la médiane mesurée est de quelques secondes.
- La traduction khmère est automatique et non relue.

### Configuration serveur (variables d'environnement)

| Variable | Rôle |
|---|---|
| `LYS_CHAT_ENABLED` | `1` pour activer ; toute autre valeur : 503 `unavailable` |
| `CLINE_API_KEY` | clé du fournisseur ; absente ou vide : 503 `unavailable` |
| `CLINE_BASE_URL` | optionnelle, défaut `https://api.cline.bot/api/v1/chat/completions` (utile pour un faux serveur de test) |
| `LYS_CHAT_DAILY_PER_USER` | optionnelle, défaut `30` |
| `LYS_CHAT_DAILY_GLOBAL` | optionnelle, défaut `300` |

## Historique du chat

Historique **partagé** entre les comptes de la liste blanche : chacun voit les échanges de l'autre. Mêmes garde (401 sans jeton, 403 hors liste blanche), limite de corps et rate-limit par IP que le reste de `/lys`. Le plafond journalier du chat ne s'applique pas à ces deux routes. Erreurs au format `{ "error", "kind" }`.

**Stockage** : un objet JSON par échange dans le bucket privé des documents (mêmes variables `LYS_S3_*`), clé `chat/AAAA/MM/JJ/<uuid>.json`, jour de **Paris**. Contenu : `{ id, created_at, author_sub, question, answer_fr, answer_km, task_slugs }`. `author_sub` n'est jamais renvoyé par l'API. Sans bucket configuré, rien n'est enregistré et la lecture renvoie une liste vide. Ni question ni réponse ne sont écrites dans les journaux.

**Rétention** : illimitée, jusqu'à suppression manuelle (`DELETE`). Aucune purge automatique.

`id` public : `AAAA-MM-JJ_<uuid>` (uuid en minuscules, forme canonique) ; il sert à reconstruire la clé. Toute autre forme est refusée avant tout accès au stockage.

### GET /lys/chat/history?limit=N

`limit` : défaut et maximum `50` (borné à 1..50 ; non numérique : `400 invalid`). `200`, du plus récent au plus ancien :

```json
{
  "items": [
    {
      "id": "2026-10-28_0b9e1c52-3f0a-4c1e-9a55-6d2f7a1b8c34",
      "created_at": "2026-10-28T10:00:00Z",
      "question": "Par quoi commencer ?",
      "answer_fr": "…",
      "answer_km": null,
      "task_slugs": ["vls-ts-anef"],
      "mine": true
    }
  ]
}
```

- `mine` : l'échange a été posé par le compte du jeton.
- **Limite de lecture : 30 jours.** Le serveur liste les jours en remontant depuis aujourd'hui (Paris) jusqu'à avoir `N` échanges, sans dépasser 30 jours en arrière ; un échange plus ancien reste stocké mais n'est plus listé.
- Un objet illisible ou corrompu est ignoré (avertissement sans contenu).
- Stockage non configuré : `200 {"items": []}`. Stockage en panne : `503 unavailable`.

### DELETE /lys/chat/history/{id}

`204` sans corps. `400 invalid` si l'`id` n'a pas la forme exacte ci-dessus ; `404 not_found` si l'objet n'existe pas ; `503 unavailable` si le stockage est absent ou en panne. N'importe lequel des comptes autorisés peut supprimer n'importe quel échange. L'objet est supprimé du bucket, qui est versionné : la version reste récupérable côté administration.
