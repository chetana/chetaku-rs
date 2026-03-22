# chetaku-rs

API REST en Rust — backend public et admin du portfolio de **Chetana YIN** : médiathèque, Strava, voyages, blog, projets, expériences, skills.

**Consommé par** :
- [chetana.dev](https://chetana.dev) — portfolio (blog, projets, CV, skills, commentaires)
- [chetana.dev/passions](https://chetana.dev/passions) — Médiathèque, Vélo, Natation, Course, Voyage
- [admin.chetana.dev](https://admin.chetana.dev) — backoffice (via proxy `chetana-admin`)

**URL custom** : `https://api.chetana.dev`
**URL Cloud Run** : `https://chetaku-rs-267131866578.europe-west1.run.app`

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
| `/health` | GET | Healthcheck |
| `/media` | GET | Liste des entrées médiathèque (filtrables par `type` et `status`) |
| `/media/{media_type}/{external_id}` | GET | Détail d'une entrée |
| `/stats` | GET | Statistiques agrégées médiathèque |
| `/strava/activities` | GET | Liste des sorties Strava (filtrables par `sport`) |
| `/strava/stats` | GET | Statistiques Strava agrégées |
| `/voyage` | GET | Liste des voyages |
| `/voyage/stats` | GET | Stats voyages — cache 30s |
| `/blog` | GET | Articles de blog publiés |
| `/blog/{slug}` | GET | Article par slug |
| `/projects` | GET | Projets du portfolio |
| `/projects/{slug}` | GET | Projet par slug |
| `/experiences` | GET | Expériences professionnelles |
| `/skills` | GET | Skills groupés par catégorie |
| `/comments/{post_id}` | GET | Commentaires approuvés d'un article |
| `/comments` | POST | Soumettre un commentaire (anti-spam) |
| `/messages` | POST | Formulaire de contact (honeypot) |

### Protégés (header `x-api-key`)

| Endpoint | Méthode | Description |
|---|---|---|
| `/sync/anime` | POST | Sync animés depuis Jikan (MyAnimeList) |
| `/sync/game` | POST | Sync jeux depuis RAWG |
| `/sync/movie` | POST | Sync films depuis TMDB |
| `/sync/series` | POST | Sync séries depuis TMDB |
| `/strava/sync` | POST | Sync activités Strava |
| `/media/{id}` | PATCH/DELETE | Update/supprime une entrée médiathèque |
| `/voyage` | POST | Crée un voyage |
| `/voyage/{id}` | PATCH/DELETE | Update/supprime un voyage |
| `/blog` | POST | Crée un article |
| `/blog/{slug}` | PATCH/DELETE | Update/supprime un article |
| `/projects` | POST | Crée un projet |
| `/projects/{slug}` | PATCH/DELETE | Update/supprime un projet |
| `/experiences` | POST | Crée une expérience |
| `/experiences/{id}` | PATCH/DELETE | Update/supprime une expérience |
| `/skills` | POST | Crée un skill |
| `/skills/{id}` | PATCH/DELETE | Update/supprime un skill |

## Paramètres de requête

### `GET /media`

| Paramètre | Type | Description |
|---|---|---|
| `type` | `anime` \| `game` \| `movie` \| `series` | Filtrer par type |
| `status` | `completed` \| `watching` \| `playing` \| `dropped` \| `plan_to_watch` \| `plan_to_play` | Filtrer par statut |

### `GET /strava/activities` et `GET /strava/stats`

| Paramètre | Type | Description |
|---|---|---|
| `sport` | `cycling` \| `running` \| `swimming` | Filtrer par type de sport — absent = toutes |

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

### Table `media_entries`

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

### Table `strava_activities`

| Colonne | Type | Description |
|---|---|---|
| `id` | bigint PK | ID Strava |
| `name` | text | Nom de la sortie |
| `sport_type` | text | `Ride`, `VirtualRide`, `Run`, `Swim`, etc. |
| `start_date` | timestamptz | Date/heure de départ |
| `distance_m` | float8 | Distance en mètres |
| `moving_time_s` | integer | Temps en mouvement (secondes) |
| `elapsed_time_s` | integer | Temps total (secondes) |
| `elevation_gain_m` | float8 | Dénivelé positif (mètres) |
| `average_speed_ms` | float8 | Vitesse moyenne (m/s) |
| `average_watts` | float8 | Puissance moyenne (watts, si capteur) |
| `average_heartrate` | float8 | FC moyenne |
| `average_cadence` | float8 | Cadence moyenne (tr/min) |
| `calories` | float8 | Calories estimées |
| `kudos_count` | integer | Nombre de kudos Strava |
| `trainer` | boolean | Sortie sur home trainer |
| `commute` | boolean | Trajet domicile-travail |
| `map_polyline` | text | Tracé encodé (Google Polyline) |
| `synced_at` | timestamptz | Dernière sync |

### Table `voyages`

| Colonne | Type | Description |
|---|---|---|
| `id` | serial PK | Identifiant interne |
| `title` | text | Titre du voyage (`"Cambodge — mars 2024"`) |
| `country_code` | char(2) | Code ISO 3166-1 alpha-2 (`"KH"`) |
| `country_name` | text | Nom du pays (`"Cambodge"`) |
| `continent` | text | Continent (`"Asie"`, `"Europe"`, etc.) |
| `date_start` | date | Début du séjour |
| `date_end` | date | Fin du séjour |
| `lat` | float8 | Latitude du centroïde (pour marqueur carte) |
| `lng` | float8 | Longitude du centroïde |
| `distance_km` | integer | Distance aller-retour estimée (km) |
| `cover_gcs_path` | text | Chemin GCS de la photo de couverture (nullable) |
| `notes` | text | Anecdote en markdown (nullable) |
| `created_at` | timestamptz | Date de création |
| `updated_at` | timestamptz | Date de dernière modification |

### Table `stats_cache`

Cache DB-persisté pour les calculs d'agrégation coûteux. TTL : 30 secondes.

| Colonne | Type | Description |
|---|---|---|
| `key` | text PK | Identifiant du cache |
| `value` | jsonb | Résultat JSON de l'agrégation |
| `computed_at` | timestamptz | Horodatage du dernier calcul |

**Clés utilisées :**
- `media_stats` — résultat de `GET /stats`
- `strava_cycling` — résultat de `GET /strava/stats?sport=cycling`
- `strava_running` — résultat de `GET /strava/stats?sport=running`
- `strava_swimming` — résultat de `GET /strava/stats?sport=swimming`
- `strava_all` — résultat de `GET /strava/stats` (sans filtre)
- `voyage_stats` — résultat de `GET /voyage/stats`

Invalidation : automatique après sync (`DELETE WHERE key LIKE 'strava_%'`) et après update/delete média (`DELETE WHERE key = 'media_stats'`).

## Variables d'environnement

| Variable | Description |
|---|---|
| `DATABASE_URL` | URL de connexion PostgreSQL (`postgres://...`) |
| `API_KEY` | Clé secrète pour les endpoints protégés |
| `RAWG_API_KEY` | Clé API RAWG (rawg.io) pour la sync jeux |
| `TMDB_API_KEY` | Clé API TMDB (themoviedb.org) pour la sync films/séries |
| `STRAVA_CLIENT_ID` | ID de l'application Strava |
| `STRAVA_CLIENT_SECRET` | Secret de l'application Strava |
| `STRAVA_REFRESH_TOKEN` | Token permanent (obtenu une seule fois via OAuth) |
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
bash deploy.sh
```

Le script `deploy.sh` lit le fichier `.env` et passe toutes les variables via `--set-env-vars` dans un seul appel `gcloud run deploy --source .` → une seule révision, env vars garanties.

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
    stats.rs       # GET /stats → agrégations médiathèque + cache DB
    sync.rs        # POST /sync/anime|game|movie|series (protégés)
    cycling.rs     # GET /strava/activities|stats, POST /strava/sync
    voyage.rs      # GET/POST/PATCH/DELETE /voyage
    update.rs      # PATCH/DELETE /media/{id} (protégés)
    blog.rs        # GET /blog, GET /blog/{slug}
    portfolio.rs   # GET /projects, /experiences, /skills
    contact.rs     # GET/POST /comments, POST /messages
    admin.rs       # CRUD protégés blog, projects, experiences, skills
  sync/
    jikan.rs       # Jikan API v4 (MyAnimeList) → AnimeData
    rawg.rs        # RAWG API v1 → GameData
    tmdb.rs        # TMDB API → MovieData, SeriesData
    strava.rs      # Strava API → get_access_token() + fetch_all_activities()
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
