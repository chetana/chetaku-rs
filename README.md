# chetaku-rs

API REST en Rust pour la médiathèque personnelle de **Chetana YIN** — suivi d'animés, de jeux vidéo, de films et de séries.

**Consommé par** : [chetana.dev/passions/medialist](https://chetana.dev/passions/medialist)

**Live** : `https://chetaku-rs-267131866578.europe-west1.run.app`

## Stack

- **Axum 0.8** — Framework HTTP async
- **Tokio** — Runtime async Rust
- **sqlx 0.8** — Requêtes PostgreSQL async type-safe
- **Cloud Run** (Google Cloud) — Hébergement serverless
- **Neon PostgreSQL** — Base de données

## Endpoints

### Publics

| Endpoint | Méthode | Description |
|---|---|---|
| `/health` | GET | Healthcheck `{"status":"ok","service":"chetaku-rs"}` |
| `/media` | GET | Liste des entrées (filtrables par `type` et `status`) |
| `/media/{media_type}/{external_id}` | GET | Détail d'une entrée |
| `/stats` | GET | Statistiques agrégées (genres, scores, studios, etc.) |

### Protégés (header `x-api-key`)

| Endpoint | Méthode | Description |
|---|---|---|
| `/sync/anime` | POST | Synchronise des animés depuis Jikan (MyAnimeList) |
| `/sync/game` | POST | Synchronise des jeux depuis RAWG |
| `/sync/movie` | POST | Synchronise des films depuis TMDB |
| `/sync/series` | POST | Synchronise des séries depuis TMDB |
| `/media/{id}` | PATCH | Met à jour une entrée (status, score, notes, etc.) |
| `/media/{id}` | DELETE | Supprime une entrée |

## Paramètres de requête

### `GET /media`

| Paramètre | Type | Description |
|---|---|---|
| `type` | `anime` \| `game` \| `movie` \| `series` | Filtrer par type |
| `status` | `completed` \| `watching` \| `playing` \| `dropped` \| `plan_to_watch` \| `plan_to_play` | Filtrer par statut |

### `POST /sync/anime`

```json
{
  "mal_ids": [1, 5114, 11061],
  "status": "completed"
}
```

### `POST /sync/game`

```json
{
  "rawg_ids": [3498, 4200],
  "status": "completed",
  "platform": "PC"
}
```

### `POST /sync/movie`

```json
{
  "tmdb_ids": [550, 157336],
  "status": "completed"
}
```

### `POST /sync/series`

```json
{
  "tmdb_ids": [1396, 60625],
  "status": "completed"
}
```

### `PATCH /media/{id}`

```json
{
  "status": "completed",
  "score": 9,
  "episodes_watched": 24,
  "notes": "Chef d'oeuvre"
}
```

Tous les champs sont optionnels — seuls les champs fournis sont mis à jour.

### `DELETE /media/{id}`

Supprime l'entrée. Requiert `x-api-key`.

**Réponse 200 :**
```json
{ "deleted": true, "id": 42 }
```

## Base de données

Une table principale `media_entries` :

| Colonne | Type | Description |
|---|---|---|
| `id` | serial PK | Identifiant interne |
| `media_type` | text | `anime`, `game`, `movie` ou `series` |
| `external_id` | integer | ID MAL (anime), RAWG (game) ou TMDB (movie/series) |
| `title` | text | Titre FR/EN |
| `title_original` | text | Titre original (japonais, etc.) |
| `status` | text | Statut de visionnage/jeu |
| `score` | smallint | Note personnelle 1–10 (nullable) |
| `episodes_watched` | integer | Épisodes vus (anime/series) |
| `episodes_total` | integer | Total épisodes (nullable) |
| `playtime_hours` | integer | Heures de jeu (game) |
| `platform` | text | Plateforme (game, nullable) |
| `cover_url` | text | URL de la jaquette |
| `genres` | text[] | Tableau de genres |
| `creator` | text | Studio (anime), développeur (game), réalisateur (movie) ou créateur (series) |
| `year` | integer | Année de sortie |
| `notes` | text | Notes personnelles |
| `synced_at` | timestamptz | Dernière sync depuis l'API externe |
| `created_at` | timestamptz | Date d'ajout |

Contrainte d'unicité : `(media_type, external_id)` — pas de doublons.

Pas de migration ALTER TABLE nécessaire pour les nouveaux types — `media_type` est stocké en TEXT, les nouvelles valeurs fonctionnent directement.

## Variables d'environnement

| Variable | Description |
|---|---|
| `DATABASE_URL` | URL de connexion PostgreSQL (`postgres://...`) |
| `API_KEY` | Clé secrète pour les endpoints protégés |
| `RAWG_API_KEY` | Clé API RAWG (rawg.io) pour la sync jeux |
| `TMDB_API_KEY` | Clé API TMDB (themoviedb.org) pour la sync films/séries |
| `PORT` | Port d'écoute (défaut : 8080) |

## Setup local

```bash
# Cloner le repo
git clone https://github.com/chetana/chetaku-rs.git
cd chetaku-rs

# Copier les variables d'environnement
cp .env.example .env
# Editer .env avec DATABASE_URL, API_KEY, RAWG_API_KEY, TMDB_API_KEY

# Lancer les migrations
# (les migrations sont appliquées automatiquement au démarrage)

# Build et lancer
cargo run

# Build release
cargo build --release
```

## Déploiement (Cloud Run)

```bash
gcloud run deploy chetaku-rs \
  --source . \
  --region europe-west1 \
  --allow-unauthenticated
```

Les variables d'environnement sont configurées dans Cloud Run via Secret Manager ou directement dans la console.

## Structure du projet

```
src/
  main.rs          # Point d'entrée, router Axum, configuration CORS
  db.rs            # Pool de connexion + migrations sqlx
  error.rs         # AppError (Db, NotFound, ExternalApi, Unauthorized) → réponses HTTP
  models.rs        # MediaEntry, MediaType, MediaStatus, SyncPayloads
  routes/
    health.rs      # GET /health
    media.rs       # GET /media, GET /media/{type}/{id}
    stats.rs       # GET /stats → agrégations complètes
    sync.rs        # POST /sync/anime, /sync/game, /sync/movie, /sync/series (protégés)
    update.rs      # PATCH /media/{id} (protégé)
    delete.rs      # DELETE /media/{id} (protégé)
  sync/
    jikan.rs       # Jikan API v4 (MyAnimeList) → AnimeData
    rawg.rs        # RAWG API v1 → GameData
    tmdb.rs        # TMDB API → MovieData, SeriesData
migrations/        # Migrations SQL (appliquées au démarrage)
```

## Statistiques (`GET /stats`)

L'endpoint `/stats` retourne des agrégations complètes :

```json
{
  "total_anime": 150,
  "total_games": 45,
  "total_movies": 30,
  "total_series": 12,
  "anime_completed": 120,
  "games_completed": 30,
  "movies_completed": 25,
  "series_completed": 8,
  "anime_watching": 8,
  "games_playing": 3,
  "average_anime_score": 7.8,
  "average_game_score": 8.1,
  "average_movie_score": 7.5,
  "average_series_score": 8.3,
  "total_episodes_watched": 3450,
  "total_playtime_hours": 580,
  "top_genres": [...],
  "top_anime_genres": [{ "genre": "Action", "count": 45, "avg_score": 8.2, "love_score": 369.0 }],
  "top_game_genres": [...],
  "top_movie_genres": [...],
  "top_series_genres": [...],
  "anime_score_distribution": [{ "score": 10, "count": 12 }],
  "game_score_distribution": [...],
  "movie_score_distribution": [...],
  "series_score_distribution": [...],
  "anime_status": [{ "status": "completed", "count": 120 }],
  "game_status": [...],
  "movie_status": [...],
  "series_status": [...],
  "top_anime_studios": [{ "creator": "Madhouse", "count": 8, "avg_score": 9.1 }],
  "top_game_developers": [...],
  "top_movie_directors": [...],
  "top_series_creators": [...]
}
```

**`love_score`** = `count × avg_score` — métrique qui équilibre fréquence et qualité pour classer les préférences de genre.

**`total_episodes_watched`** comptabilise les épisodes vus pour les animés et les séries.

## Documentation

- [API](docs/API.md) — Détail des endpoints, formats, erreurs
- [Architecture](docs/ARCHITECTURE.md) — Choix techniques, flux de données

## Licence

MIT
