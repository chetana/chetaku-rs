# API — chetaku-rs

Base URL : `https://chetaku-rs-267131866578.europe-west1.run.app`

## Authentification

Les endpoints en lecture sont **publics**. Les endpoints d'écriture et de sync requièrent un header :

```
x-api-key: <API_KEY>
```

Réponse si clé absente ou invalide : `401 Unauthorized`

---

## GET /health

Vérifie que le service est en ligne.

**Réponse 200 :**
```json
{ "status": "ok", "service": "chetaku-rs" }
```

---

## GET /media

Liste les entrées de la médiathèque.

**Paramètres query :**

| Paramètre | Requis | Valeurs |
|---|---|---|
| `type` | non | `anime`, `game`, `movie`, `series` |
| `status` | non | `completed`, `watching`, `playing`, `dropped`, `plan_to_watch`, `plan_to_play` |

**Exemples :**
```
GET /media                           → toutes les entrées
GET /media?type=anime                → tous les animés
GET /media?type=movie                → tous les films
GET /media?type=game&status=playing  → jeux en cours
```

**Réponse 200 :**
```json
[
  {
    "id": 1,
    "media_type": "anime",
    "external_id": 5114,
    "title": "Fullmetal Alchemist: Brotherhood",
    "title_original": "鋼の錬金術師 FULLMETAL ALCHEMIST",
    "status": "completed",
    "score": 10,
    "episodes_watched": 64,
    "episodes_total": 64,
    "playtime_hours": null,
    "platform": null,
    "cover_url": "https://cdn.myanimelist.net/images/anime/...",
    "genres": ["Action", "Adventure", "Drama", "Fantasy"],
    "creator": "Bones",
    "year": 2009,
    "notes": "Masterpiece",
    "synced_at": "2025-01-15T10:00:00Z",
    "created_at": "2025-01-15T10:00:00Z"
  }
]
```

---

## GET /media/{media_type}/{external_id}

Récupère une entrée par son type et son ID externe (MAL ID, RAWG ID ou TMDB ID).

**Exemple :**
```
GET /media/anime/5114
GET /media/game/3498
GET /media/movie/550
GET /media/series/1396
```

**Réponse 200 :** Objet `MediaEntry` (même format que la liste)

**Réponse 404 :**
```json
{ "error": "not found" }
```

---

## GET /stats

Retourne des statistiques agrégées sur toute la médiathèque.

**Réponse 200 :**
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
  "average_anime_score": 7.84,
  "average_game_score": 8.12,
  "average_movie_score": 7.50,
  "average_series_score": 8.30,
  "total_episodes_watched": 3450,
  "total_playtime_hours": 580,

  "top_genres": [
    { "genre": "Action", "count": 65 }
  ],

  "top_anime_genres": [
    { "genre": "Action", "count": 45, "avg_score": 8.2, "love_score": 369.0 }
  ],

  "top_game_genres": [
    { "genre": "RPG", "count": 12, "avg_score": 8.8, "love_score": 105.6 }
  ],

  "top_movie_genres": [
    { "genre": "Drama", "count": 10, "avg_score": 7.8, "love_score": 78.0 }
  ],

  "top_series_genres": [
    { "genre": "Crime", "count": 5, "avg_score": 9.0, "love_score": 45.0 }
  ],

  "anime_score_distribution": [
    { "score": 8, "count": 45 },
    { "score": 9, "count": 28 },
    { "score": 10, "count": 12 }
  ],

  "game_score_distribution": [
    { "score": 8, "count": 15 }
  ],

  "movie_score_distribution": [
    { "score": 7, "count": 8 }
  ],

  "series_score_distribution": [
    { "score": 9, "count": 4 }
  ],

  "anime_status": [
    { "status": "completed", "count": 120 },
    { "status": "watching", "count": 8 }
  ],

  "game_status": [
    { "status": "completed", "count": 30 }
  ],

  "movie_status": [
    { "status": "completed", "count": 25 }
  ],

  "series_status": [
    { "status": "completed", "count": 8 }
  ],

  "top_anime_studios": [
    { "creator": "Madhouse", "count": 8, "avg_score": 9.1 }
  ],

  "top_game_developers": [
    { "creator": "FromSoftware", "count": 4, "avg_score": 9.5 }
  ],

  "top_movie_directors": [
    { "creator": "Christopher Nolan", "count": 3, "avg_score": 9.2 }
  ],

  "top_series_creators": [
    { "creator": "Vince Gilligan", "count": 2, "avg_score": 9.8 }
  ]
}
```

### Champs notables

- **`top_genres`** — Top 10 genres toutes catégories confondues, triés par `count`
- **`top_anime_genres` / `top_game_genres` / `top_movie_genres` / `top_series_genres`** — Top 10 genres pondérés par score, uniquement sur les entrées notées. `love_score = count × avg_score` (équilibre fréquence et qualité)
- **`top_anime_studios` / `top_game_developers` / `top_movie_directors` / `top_series_creators`** — Top 6 créateurs, triés par nombre d'entrées puis par note moyenne
- **`total_episodes_watched`** — comptabilise anime + series

---

## POST /sync/anime

Synchronise des animés depuis l'API Jikan (MyAnimeList). Requiert `x-api-key`.

**Body :**
```json
{
  "mal_ids": [1, 5114, 11061],
  "status": "completed"
}
```

| Champ | Requis | Défaut |
|---|---|---|
| `mal_ids` | oui | — |
| `status` | non | `"completed"` |

**Comportement :**
- Pour chaque `mal_id`, appelle `https://api.jikan.moe/v4/anime/{id}/full`
- Rate limiting : 400 ms entre chaque appel (limite Jikan : 3 req/sec)
- Upsert dans `media_entries` (conflit sur `(media_type, external_id)` → update)
- Les erreurs individuelles sont loggées mais n'interrompent pas le batch

**Réponse 200 :**
```json
{ "synced": 3, "total": 3 }
```

---

## POST /sync/game

Synchronise des jeux depuis l'API RAWG. Requiert `x-api-key`.

**Body :**
```json
{
  "rawg_ids": [3498, 4200],
  "status": "completed",
  "platform": "PC"
}
```

| Champ | Requis | Défaut |
|---|---|---|
| `rawg_ids` | oui | — |
| `status` | non | `"completed"` |
| `platform` | non | `null` |

**Réponse 200 :**
```json
{ "synced": 2, "total": 2 }
```

---

## POST /sync/movie

Synchronise des films depuis l'API TMDB. Requiert `x-api-key`.

**Body :**
```json
{
  "tmdb_ids": [550, 157336],
  "status": "completed"
}
```

| Champ | Requis | Défaut |
|---|---|---|
| `tmdb_ids` | oui | — |
| `status` | non | `"completed"` |

**Comportement :**
- Pour chaque `tmdb_id`, appelle `GET /movie/{id}` + `GET /movie/{id}/credits` (pour le réalisateur)
- Cover : `https://image.tmdb.org/t/p/w500{poster_path}`
- Langue : `language=fr-FR` pour les titres et synopsis localisés
- Upsert dans `media_entries`

**Réponse 200 :**
```json
{ "synced": 2, "total": 2 }
```

---

## POST /sync/series

Synchronise des séries depuis l'API TMDB. Requiert `x-api-key`.

**Body :**
```json
{
  "tmdb_ids": [1396, 60625],
  "status": "completed"
}
```

| Champ | Requis | Défaut |
|---|---|---|
| `tmdb_ids` | oui | — |
| `status` | non | `"completed"` |

**Comportement :**
- Pour chaque `tmdb_id`, appelle `GET /tv/{id}`
- Extrait : créateur (`created_by[0].name`), nombre de saisons, total d'épisodes
- Cover : `https://image.tmdb.org/t/p/w500{poster_path}`
- Langue : `language=fr-FR`

**Réponse 200 :**
```json
{ "synced": 2, "total": 2 }
```

---

## PATCH /media/{id}

Met à jour une entrée existante. Requiert `x-api-key`.

L'`id` est l'identifiant interne PostgreSQL (pas le MAL ID, RAWG ID ou TMDB ID).

**Body (tous les champs optionnels) :**
```json
{
  "status": "completed",
  "score": 9,
  "episodes_watched": 24,
  "playtime_hours": 80,
  "platform": "PS5",
  "notes": "Excellent jeu"
}
```

**Réponse 200 (succès) :**
```json
{ "updated": true, "id": 42 }
```

**Réponse 200 (aucun champ fourni) :**
```json
{ "updated": false, "reason": "no fields provided" }
```

**Réponse 404 :**
```json
{ "error": "not found" }
```

---

## DELETE /media/{id}

Supprime une entrée. Requiert `x-api-key`.

L'`id` est l'identifiant interne PostgreSQL.

**Réponse 200 :**
```json
{ "deleted": true, "id": 42 }
```

**Réponse 404 :**
```json
{ "error": "not found" }
```

---

---

## GET /strava/activities

Liste les sorties Strava depuis la DB.

**Paramètres query :**

| Paramètre | Requis | Valeurs |
|---|---|---|
| `sport` | non | `cycling`, `running`, `swimming` — absent = toutes |

**Exemples :**
```
GET /strava/activities              → toutes les activités
GET /strava/activities?sport=cycling → sorties vélo
GET /strava/activities?sport=running → sorties course
```

**Réponse 200 :**
```json
[
  {
    "id": 12345678,
    "name": "Sortie matinale",
    "sport_type": "Ride",
    "start_date": "2025-04-15T06:30:00Z",
    "distance_m": 45200.5,
    "moving_time_s": 5400,
    "elapsed_time_s": 5800,
    "elevation_gain_m": 320.0,
    "average_speed_ms": 8.37,
    "max_speed_ms": 14.2,
    "average_watts": 185.0,
    "average_heartrate": 148.0,
    "max_heartrate": 172.0,
    "average_cadence": 88.0,
    "calories": 980.0,
    "kudos_count": 5,
    "pr_count": 1,
    "trainer": false,
    "commute": false,
    "map_polyline": "abcdef...",
    "synced_at": "2025-04-15T10:00:00Z"
  }
]
```

---

## GET /strava/stats

Retourne des statistiques agrégées des activités Strava. Résultat mis en cache dans `stats_cache` (TTL 30s).

**Paramètres query :**

| Paramètre | Requis | Valeurs |
|---|---|---|
| `sport` | non | `cycling`, `running`, `swimming` — absent = toutes |

**Réponse 200 :**
```json
{
  "total_rides": 275,
  "total_km": 4936.0,
  "total_elevation_m": 32450.0,
  "total_moving_time_s": 648000,
  "best_ride_km": 100.1,
  "best_elevation_m": 363.5,
  "average_km_per_ride": 17.9,
  "monthly": [
    { "month": "2025-04", "km": 53.3, "elevation_m": 234.0, "rides": 1 }
  ],
  "by_sport_type": [
    { "sport_type": "Ride", "count": 250, "km": 4500.0 },
    { "sport_type": "VirtualRide", "count": 25, "km": 436.0 }
  ],
  "top_rides": [
    {
      "id": 12345678,
      "name": "Gran Fondo",
      "start_date": "2025-06-01T07:00:00Z",
      "distance_m": 100112.0,
      "elevation_gain_m": 363.5,
      "moving_time_s": 12600,
      "average_speed_ms": 7.95
    }
  ]
}
```

`monthly` couvre les 12 derniers mois. `top_rides` = top 5 par distance.

---

## POST /strava/sync

Synchronise toutes les activités depuis l'API Strava (pagination complète). Requiert `x-api-key`.

**Body :** aucun

**Comportement :**
- Rafraîchit l'access token via le `STRAVA_REFRESH_TOKEN`
- Pagine `GET /athlete/activities?per_page=200&page=N` jusqu'à réponse vide
- 400ms entre chaque page (rate limit Strava : 100 req/15min)
- Upsert dans `strava_activities` (conflit sur `id`)
- Après sync : invalide toutes les clés `strava_%` de `stats_cache`

**Réponse 200 :**
```json
{ "synced": 275, "total": 275 }
```

---

---

## GET /voyage

Liste tous les voyages, triés par `date_start DESC`.

**Réponse 200 :**
```json
[
  {
    "id": 1,
    "title": "Cambodge — mars 2024",
    "country_code": "KH",
    "country_name": "Cambodge",
    "continent": "Asie",
    "date_start": "2024-03-10",
    "date_end": "2024-03-25",
    "lat": 11.55,
    "lng": 104.9167,
    "distance_km": 17500,
    "cover_gcs_path": "voyages/cambodge-2024/cover.jpg",
    "notes": "Angkor Wat au lever du soleil.",
    "created_at": "2025-01-01T00:00:00Z"
  }
]
```

---

## GET /voyage/stats

Statistiques agrégées des voyages. Résultat mis en cache (`voyage_stats`, TTL 30s).

**Réponse 200 :**
```json
{
  "total_trips": 12,
  "total_countries": 8,
  "total_km": 145000,
  "continents": ["Asie", "Europe"],
  "by_year": [
    { "year": 2024, "trips": 3 },
    { "year": 2023, "trips": 2 }
  ]
}
```

---

## POST /voyage

Crée un voyage. Requiert `x-api-key`.

**Body :**
```json
{
  "title": "Cambodge — mars 2024",
  "country_code": "KH",
  "country_name": "Cambodge",
  "continent": "Asie",
  "date_start": "2024-03-10",
  "date_end": "2024-03-25",
  "lat": 11.55,
  "lng": 104.9167,
  "distance_km": 17500,
  "cover_gcs_path": "voyages/cambodge-2024/cover.jpg",
  "notes": "Angkor Wat au lever du soleil."
}
```

`cover_gcs_path` et `notes` sont optionnels.

**Réponse 200 :**
```json
{ "created": true, "id": 1 }
```

---

## PATCH /voyage/{id}

Met à jour un voyage. Requiert `x-api-key`. Tous les champs sont optionnels.

**Body :**
```json
{
  "title": "Cambodge — mars 2024 ✨",
  "notes": "Texte mis à jour",
  "cover_gcs_path": "voyages/cambodge-2024/cover-v2.jpg",
  "distance_km": 18000
}
```

**Réponse 200 :**
```json
{ "updated": true, "id": 1 }
```

---

## DELETE /voyage/{id}

Supprime un voyage. Requiert `x-api-key`.

**Réponse 200 :**
```json
{ "deleted": true, "id": 1 }
```

**Réponse 404 :**
```json
{ "error": "not found" }
```

---

## Codes d'erreur

| Code | Description |
|---|---|
| `200` | Succès |
| `401` | `x-api-key` absent ou invalide |
| `404` | Ressource introuvable |
| `500` | Erreur base de données |
| `502` | Erreur API externe (Jikan, RAWG, TMDB) |

Format d'erreur :
```json
{ "error": "description de l'erreur" }
```
