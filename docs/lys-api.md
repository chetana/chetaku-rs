# API lys-en-france (`/lys`)

Base : `https://chetaku.chetana.fr`. Toutes les routes exigent `Authorization: Bearer <jeton Logto>` d'audience `https://lys-en-france.chetana.fr/api` et un `sub` de la liste blanche. JSON UTF-8. CORS : origines `https://lys-en-france.chetana.fr` et `http://localhost:5173`. Corps limité à 16 Kio (sauf l'envoi de document, 12 Mio).

## Erreurs (toutes les routes)

Format unique : `{ "error": "<message>" }`.

| Code | Cas |
|---|---|
| 400 | identifiant de document qui n'est pas un UUID canonique ; fichier vide ; JSON mal formé ; valeur refusée (statut hors liste, note > 2000 caractères ou avec caractère de contrôle autre que `\n`, corps PATCH vide) |
| 401 | jeton absent, invalide, expiré, mauvaise audience |
| 403 | jeton valide, `sub` hors liste blanche (`{"error":"Forbidden"}`) : le front affiche la page « changer de compte » |
| 404 | slug inconnu (`{"error":"Not found"}`) |
| 413 | corps > 16 Kio (document : > 12 Mio) |
| 415 | document dont le type réel n'est pas PDF, JPEG, PNG ou HEIC/HEIF |
| 422 | JSON valide mais champ inconnu, manquant ou de mauvais type (ex. date illisible) |
| 429 | limite par IP (120 req/min) |
| 502 | stockage des documents en erreur (`{"error":"stockage en erreur"}`, détail dans les logs seulement) |
| 503 | stockage des documents non configuré (`{"error":"stockage indisponible"}`) ; le reste de `/lys` fonctionne |
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
          "km_status": "none",
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
