# Architecture — chetaku-rs

## Vue d'ensemble

```
┌──────────────────────────────┐    ┌──────────────────────────────────┐
│      chetana.dev (Nuxt)      │    │   admin.chetana.dev              │
│                              │    │   (chetana-admin / Cloud Run)    │
│  /passions → strava, media   │    │                                  │
│  /blog, /projects, /cv       │    │  React + Express proxy           │
│   └─ proxy → /api/*          │    │   └─ requireAuth (Google token)  │
│      (server/api/ Nuxt)      │    │   └─ fetch + x-api-key          │
└──────────────┬───────────────┘    └──────────────┬───────────────────┘
               │ HTTP public / x-api-key            │ HTTP + x-api-key
               └──────────────┬─────────────────────┘
                              ▼
              ┌─────────────────────────────────────────────┐
              │         api.chetana.dev                     │
              │    chetaku-rs · Axum · Cloud Run            │
              │                                             │
              │  Public :                                   │
              │  GET /blog, /blog/{slug}                    │
              │  GET /projects, /projects/{slug}            │
              │  GET /experiences, /skills                  │
              │  GET /comments/{post_id}                    │
              │  POST /comments, /messages                  │
              │  GET /media, /stats                         │
              │  GET /strava/activities, /strava/stats      │
              │  GET /voyage, /voyage/stats                 │
              │                                             │
              │  Protégés (x-api-key) :                     │
              │  POST/PATCH/DELETE /blog                    │
              │  POST/PATCH/DELETE /projects                │
              │  POST/PATCH/DELETE /experiences, /skills    │
              │  POST /sync/anime|game|movie|series         │
              │  POST /strava/sync                          │
              │  POST/PATCH/DELETE /voyage                  │
              │  PATCH/DELETE /media/{id}                   │
              └───────────────────┬─────────────────────────┘
                                  │
              ┌───────────────────▼──────────────────────────┐
              │              Neon PostgreSQL                  │
              │  ├── blog_posts      ├── media_entries        │
              │  ├── projects        ├── strava_activities    │
              │  ├── experiences     ├── voyages              │
              │  ├── skills          ├── comments             │
              │  ├── messages        └── stats_cache (TTL 30s)│
              │  └── (users, health_entries, push_subs…)     │
              │      ← gérés par chetana-dev (Nuxt/Drizzle)  │
              └──────────────────────────────────────────────┘

Sync sources :  Jikan(MAL) · RAWG · TMDB · Strava API
```

## Stack technique

| Couche | Technologie | Justification |
|---|---|---|
| Framework HTTP | Axum 0.8 | Ergonomique, type-safe, basé sur Tower |
| Runtime async | Tokio | Standard de facto pour l'async Rust |
| Base de données | sqlx 0.8 + Neon PostgreSQL | Requêtes async, compile-time checks |
| Hébergement | Google Cloud Run | Serverless, scale to zero, région `europe-west1` |
| Logs | tracing + tracing-subscriber | Structured logging, compatible Cloud Run |
| Erreurs | thiserror + anyhow | Conversions ergonomiques, messages clairs |
| HTTP client | reqwest 0.12 | Appels vers Jikan, RAWG et TMDB |
| Sérialisation | serde + serde_json | Standard Rust |

## Structure du code

```
chetaku-rs/
├── src/
│   ├── main.rs          # Axum router, CORS, TcpListener, tokio::main
│   ├── db.rs            # create_pool() + run_migrations()
│   ├── error.rs         # AppError (Db, NotFound, ExternalApi, Unauthorized, Validation)
│   ├── models.rs        # MediaEntry, MediaType, MediaStatus, payloads
│   ├── routes/
│   │   ├── mod.rs
│   │   ├── health.rs    # GET /health
│   │   ├── media.rs     # GET /media, GET /media/{type}/{id}
│   │   ├── stats.rs     # GET /stats → agrégations médiathèque + cache DB
│   │   ├── cycling.rs   # GET /strava/activities|stats + POST /strava/sync
│   │   ├── voyage.rs    # GET /voyage, GET /voyage/stats, POST/PATCH/DELETE /voyage
│   │   ├── sync.rs      # POST /sync/anime|game|movie|series (protégés)
│   │   ├── update.rs    # PATCH/DELETE /media/{id} (protégés)
│   │   ├── blog.rs      # GET /blog, GET /blog/{slug}
│   │   ├── portfolio.rs # GET /projects, /experiences, /skills
│   │   ├── contact.rs   # GET/POST /comments, POST /messages
│   │   └── admin.rs     # CRUD protégés blog, projects, experiences, skills
│   └── sync/
│       ├── mod.rs
│       ├── jikan.rs     # Jikan API v4 → AnimeData normalisé
│       ├── rawg.rs      # RAWG API v1 → GameData normalisé
│       ├── tmdb.rs      # TMDB API v3 → MovieData, SeriesData normalisés
│       └── strava.rs    # Strava API → get_access_token() + fetch_all_activities()
├── migrations/          # Fichiers SQL (appliqués au démarrage)
├── Cargo.toml
├── Dockerfile
├── deploy.sh            # Deploy Cloud Run avec env vars depuis .env
└── docs/
    ├── API.md
    └── ARCHITECTURE.md
```

## Gestion des erreurs

`AppError` (src/error.rs) mappe les erreurs internes vers des réponses HTTP JSON :

| Variant | HTTP | Cas |
|---|---|---|
| `AppError::Db(sqlx::Error)` | 500 | Erreur base de données |
| `AppError::NotFound` | 404 | Entrée inexistante |
| `AppError::ExternalApi(String)` | 502 | Jikan, RAWG ou TMDB inaccessible |
| `AppError::Unauthorized` | 401 | Clé API absente ou invalide |
| `AppError::Validation(String)` | 400 | Champ requis manquant ou invalide |

Toutes les routes retournent `Result<Json<T>, AppError>`. L'implémentation `IntoResponse` d'`AppError` transforme automatiquement les erreurs en réponse `{ "error": "..." }`.

## Authentification

Les endpoints en lecture (`GET /media`, `GET /stats`) sont publics — pas d'auth requise.

Les endpoints d'écriture (`POST /sync/*`, `PATCH /media/{id}`, `DELETE /media/{id}`) lisent le header `x-api-key` et le comparent à la variable d'environnement `API_KEY`.

## CORS

Configuré dans `main.rs` via `tower_http::cors::CorsLayer` :

```rust
CorsLayer::new()
    .allow_origin([
        "https://chetana.dev",
        "https://chetlys.vercel.app",
        "http://localhost:3000",
        "http://localhost:5173",
    ])
```

Autorise chetana.dev, chet_lys (Vercel), et les deux ports de dev local (Nuxt/Vite).

## Migrations

Les migrations SQL sont dans `migrations/` et appliquées au démarrage du service via `sqlx::migrate!()`. Format : `{timestamp}_{description}.sql`.

Pas de migration ALTER TABLE nécessaire pour les types `movie` et `series` — la colonne `media_type` est de type TEXT, les nouvelles valeurs (`'movie'`, `'series'`) sont insérées directement sans modifier le schéma.

## Synchronisation Jikan (MyAnimeList)

`src/sync/jikan.rs` appelle `https://api.jikan.moe/v4/anime/{mal_id}/full`.

Normalisation :
- Image : `images.jpg.large_image_url` (fallback sur `image_url`)
- Genres : `genres[].name` (tableau de strings)
- Studio : `studios[0].name` (premier studio uniquement)
- Année : `year` (peut être null si en cours de diffusion)

Rate limiting : `tokio::time::sleep(Duration::from_millis(400))` entre chaque appel pour respecter la limite de 3 req/sec de Jikan.

Upsert SQL :
```sql
INSERT INTO media_entries (...) VALUES (...)
ON CONFLICT (media_type, external_id)
DO UPDATE SET title = EXCLUDED.title, ...
```

## Synchronisation RAWG

`src/sync/rawg.rs` appelle `https://api.rawg.io/api/games/{rawg_id}?key={RAWG_API_KEY}`.

Normalisation :
- Genres : `genres[].name`
- Développeur : `developers[0].name` (premier développeur uniquement)
- Année : extrait des 4 premiers caractères de `released` (`"YYYY-MM-DD"`)

## Synchronisation TMDB

`src/sync/tmdb.rs` appelle l'API TMDB v3 (`api.themoviedb.org/3`) avec `?api_key={TMDB_API_KEY}&language=fr-FR`.

### Films (`/sync/movie`)

Pour chaque TMDB ID :
- `GET /movie/{id}` — titre, synopsis, genres, année, runtime, tagline
- `GET /movie/{id}/credits` — directeur (premier `crew` avec `job == "Director"`)
- Cover : `https://image.tmdb.org/t/p/w500{poster_path}`

### Séries (`/sync/series`)

Pour chaque TMDB ID :
- `GET /tv/{id}` — titre, synopsis, genres, année, créateur (`created_by[0].name`), nombre de saisons, total d'épisodes
- Cover : `https://image.tmdb.org/t/p/w500{poster_path}`

TMDB a été choisi pour les films et séries — seule API gratuite viable offrant des métadonnées complètes en français.

## Agrégations stats (love_score)

La formule `love_score = count × avg_score` équilibre :
- **Fréquence** : un genre présent dans 50 animés a plus de poids qu'un genre dans 2
- **Qualité** : un genre avec une note moyenne de 9 est préféré à un genre noté 5

SQL (avec cast `::float8` obligatoire — sqlx sans feature `bigdecimal` ne peut pas décoder `NUMERIC`) :

```sql
SELECT genre,
       COUNT(*) as count,
       ROUND(AVG(score::float)::numeric, 2)::float8 as avg_score,
       ROUND((COUNT(*) * AVG(score::float))::numeric, 2)::float8 as love_score
FROM media_entries, UNNEST(genres) AS genre
WHERE media_type = 'anime' AND score IS NOT NULL
GROUP BY genre ORDER BY love_score DESC LIMIT 10
```

Les mêmes agrégations sont calculées pour `movie` et `series` (top genres, score distribution, statuts, créateurs).

`total_episodes_watched` agrège `episodes_watched` pour `media_type IN ('anime', 'series')`.

## Cache DB (`stats_cache`)

Les agrégations de stats sont coûteuses (10+ requêtes SQL en parallèle). Pour éviter de les recalculer à chaque visite — et puisque Cloud Run scale à zéro (pas d'état en mémoire persistant) — les résultats sont stockés dans une table `stats_cache` (Neon PostgreSQL).

**Stratégie :**
1. Handler reçoit une requête → `SELECT value FROM stats_cache WHERE key = $1 AND computed_at > NOW() - interval '30 seconds'`
2. Si hit → retourne le JSONB stocké directement (très rapide : ~50ms)
3. Si miss (cache absent ou expiré) → calcule, sérialise en JSON, `UPSERT` dans `stats_cache`, retourne le résultat

**Clés de cache :**

| Clé | Endpoint | Invalidée par |
|---|---|---|
| `media_stats` | `GET /stats` | `PATCH /media/{id}`, `DELETE /media/{id}`, après sync |
| `strava_cycling` | `GET /strava/stats?sport=cycling` | `POST /strava/sync` |
| `strava_running` | `GET /strava/stats?sport=running` | `POST /strava/sync` |
| `strava_swimming` | `GET /strava/stats?sport=swimming` | `POST /strava/sync` |
| `strava_all` | `GET /strava/stats` | `POST /strava/sync` |
| `voyage_stats` | `GET /voyage/stats` | `POST/PATCH/DELETE /voyage` |

Invalidation Strava : `DELETE FROM stats_cache WHERE key LIKE 'strava_%'` après chaque sync réussie.

## Synchronisation Strava

`src/sync/strava.rs` :
- `get_access_token()` : POST `https://www.strava.com/api/v3/oauth/token` avec `grant_type=refresh_token` → retourne l'access token (valide 6h)
- `fetch_all_activities(access_token)` : pagination `GET /athlete/activities?per_page=200&page=N` jusqu'à réponse vide, 400ms entre pages

Sport types Strava mappés :
- `cycling` → `[Ride, VirtualRide, MountainBikeRide, GravelRide, EBikeRide, Velomobile]`
- `running` → `[Run, TrailRun, VirtualRun]`
- `swimming` → `[Swim, OpenWaterSwim]`

Obtention du `refresh_token` (une seule fois) :
```
1. GET https://www.strava.com/oauth/authorize?client_id=...&scope=activity:read_all&...
2. Autoriser → récupérer code= dans l'URL
3. POST /oauth/token avec code + grant_type=authorization_code → refresh_token permanent
```

## Déploiement Cloud Run

```bash
gcloud run deploy chetaku-rs \
  --source . \
  --region europe-west1 \
  --allow-unauthenticated
```

Cloud Run build automatiquement l'image Docker depuis le `Cargo.toml` (buildpacks GCP ou Dockerfile). Le service scale à zéro quand inactif.

Variables d'environnement configurées dans Cloud Run :
- `DATABASE_URL` — URL Neon PostgreSQL
- `API_KEY` — Clé secrète pour les endpoints protégés
- `RAWG_API_KEY` — Clé API RAWG
- `TMDB_API_KEY` — Clé API TMDB
- `STRAVA_CLIENT_ID` — ID de l'application Strava
- `STRAVA_CLIENT_SECRET` — Secret de l'application Strava
- `STRAVA_REFRESH_TOKEN` — Token permanent Strava OAuth

⚠️ `gcloud run deploy --source` remet les env vars à zéro : toujours relancer `gcloud run services update --update-env-vars KEY=VALUE,...` après un redéploiement depuis les sources.

URL du service : `https://chetaku-rs-267131866578.europe-west1.run.app`
