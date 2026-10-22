//! `GET /automations/status` : dernier passage de chaque timer et état « à jour / en cours / en retard ».
//! Les calendriers recopient `chet-workspace/home-automations/units/*.timer` (heure de Paris).

use axum::{extract::State, http::HeaderMap, Json};
use chrono::{DateTime, Datelike, Duration, Timelike, Utc, Weekday};
use chrono_tz::Europe::Paris;
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::error::AppError;

pub struct Sched {
    pub weekdays_only: bool,
    pub hours: &'static [u32],
    pub minutes: &'static [u32],
    pub grace_min: i64,
}

const ALL_HOURS: &[u32] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23];

pub const SCHEDULES: &[(&str, Sched)] = &[
    ("pod-restart-watch", Sched { weekdays_only: false, hours: ALL_HOURS, minutes: &[0], grace_min: 20 }),
    ("mr-review-watch", Sched { weekdays_only: false, hours: ALL_HOURS, minutes: &[5, 35], grace_min: 20 }),
    ("mr-feedback-watch", Sched { weekdays_only: false, hours: ALL_HOURS, minutes: &[0, 30], grace_min: 20 }),
    ("sentry-watch", Sched { weekdays_only: false, hours: &[9], minutes: &[0], grace_min: 60 }),
    ("sync-mr-tickets", Sched { weekdays_only: true, hours: &[8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19], minutes: &[20], grace_min: 20 }),
    ("logto-reconcile", Sched { weekdays_only: true, hours: &[9], minutes: &[10], grace_min: 60 }),
    ("gemini-notes-sync", Sched { weekdays_only: true, hours: &[9], minutes: &[15], grace_min: 60 }),
    ("recap-veille", Sched { weekdays_only: true, hours: &[10], minutes: &[30], grace_min: 60 }),
    ("digest-jour", Sched { weekdays_only: true, hours: &[17], minutes: &[45], grace_min: 60 }),
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Health {
    AJour,
    EnCours { due_at: DateTime<Utc> },
    EnRetard { due_at: DateTime<Utc> },
}

/// Dernière échéance ≤ `now`, cherchée minute par minute sur 8 jours (couvre un week-end et un jour férié).
pub fn last_due(now: DateTime<Utc>, s: &Sched) -> Option<DateTime<Utc>> {
    let mut t = now.with_second(0)?.with_nanosecond(0)?;
    for _ in 0..(8 * 24 * 60) {
        let local = t.with_timezone(&Paris);
        let day_ok = !s.weekdays_only || !matches!(local.weekday(), Weekday::Sat | Weekday::Sun);
        if day_ok && s.hours.contains(&local.hour()) && s.minutes.contains(&local.minute()) {
            return Some(t);
        }
        t -= Duration::minutes(1);
    }
    None
}

pub fn evaluate(now: DateTime<Utc>, s: &Sched, last_started: Option<DateTime<Utc>>) -> Health {
    let Some(due) = last_due(now, s) else { return Health::AJour };
    if last_started.is_some_and(|l| l >= due - Duration::minutes(2)) {
        return Health::AJour;
    }
    if now - due <= Duration::minutes(s.grace_min) {
        Health::EnCours { due_at: due }
    } else {
        Health::EnRetard { due_at: due }
    }
}

type LastRow = (String, i64, String, DateTime<Utc>, Option<DateTime<Utc>>, String, String);

fn describe(name: &str, health: Option<Health>, last: Option<&LastRow>) -> Value {
    let (state, due_at) = match health {
        Some(Health::AJour) => ("a_jour", None),
        Some(Health::EnCours { due_at }) => ("en_cours", Some(due_at)),
        Some(Health::EnRetard { due_at }) => ("en_retard", Some(due_at)),
        None => ("non_planifie", None),
    };
    let last = last.map(|(_, id, run_id, started, ended, status, headline)| {
        json!({ "id": id, "run_id": run_id, "started_at": started, "ended_at": ended, "status": status, "headline": headline })
    });
    json!({ "automation": name, "state": state, "due_at": due_at, "last": last })
}

pub async fn status(State(pool): State<PgPool>, headers: HeaderMap) -> Result<Json<Value>, AppError> {
    super::runs::guard(&headers).await?;
    let rows: Vec<LastRow> = sqlx::query_as(
        "SELECT DISTINCT ON (automation) automation, id, run_id, started_at, ended_at, status, headline
         FROM automation_runs ORDER BY automation, started_at DESC",
    )
    .fetch_all(&pool)
    .await?;
    let now = Utc::now();
    let mut out: Vec<Value> = SCHEDULES
        .iter()
        .map(|(name, s)| {
            let last = rows.iter().find(|r| r.0 == *name);
            describe(name, Some(evaluate(now, s, last.map(|r| r.3))), last)
        })
        .collect();
    for r in rows.iter().filter(|r| !SCHEDULES.iter().any(|(n, _)| *n == r.0)) {
        out.push(describe(&r.0, None, Some(r)));
    }
    Ok(Json(json!({ "now": now, "automations": out })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(s: &str) -> DateTime<Utc> { s.parse().unwrap() }
    fn sched(name: &str) -> &'static Sched { &SCHEDULES.iter().find(|(n, _)| *n == name).unwrap().1 }

    #[test]
    fn les_9_timers_sont_declares() {
        let mut names: Vec<_> = SCHEDULES.iter().map(|(n, _)| *n).collect();
        names.sort();
        assert_eq!(names, [
            "digest-jour", "gemini-notes-sync", "logto-reconcile", "mr-feedback-watch", "mr-review-watch",
            "pod-restart-watch", "recap-veille", "sentry-watch", "sync-mr-tickets",
        ]);
    }

    #[test]
    fn digest_jour_en_semaine_a_17h45_paris() {
        // vendredi 23/10 18:00 Paris (16:00Z) → échéance 17:45 Paris = 15:45Z ; samedi : toujours vendredi
        assert_eq!(last_due(utc("2026-10-23T16:00:00Z"), sched("digest-jour")), Some(utc("2026-10-23T15:45:00Z")));
        assert_eq!(last_due(utc("2026-10-24T12:00:00Z"), sched("digest-jour")), Some(utc("2026-10-23T15:45:00Z")));
    }

    #[test]
    fn horaire_derniere_echeance_en_heure_de_paris() {
        // 14:03Z = 16:03 Paris (été) → pod-restart-watch (toutes les heures) : 16:00 Paris = 14:00Z
        assert_eq!(last_due(utc("2026-10-22T14:03:00Z"), sched("pod-restart-watch")), Some(utc("2026-10-22T14:00:00Z")));
        // mr-review-watch :05 et :35 → 16:35 Paris = 14:35Z
        assert_eq!(last_due(utc("2026-10-22T14:40:00Z"), sched("mr-review-watch")), Some(utc("2026-10-22T14:35:00Z")));
    }

    #[test]
    fn semaine_seulement_saute_le_week_end() {
        // samedi 24/10 midi : dernier passage de sync-mr-tickets = vendredi 23/10 19:20 Paris = 17:20Z
        assert_eq!(last_due(utc("2026-10-24T12:00:00Z"), sched("sync-mr-tickets")), Some(utc("2026-10-23T17:20:00Z")));
        // lundi 26/10 07:00 Paris (06:00Z, hiver) : toujours vendredi 19:20 Paris
        assert_eq!(last_due(utc("2026-10-26T06:00:00Z"), sched("sync-mr-tickets")), Some(utc("2026-10-23T17:20:00Z")));
    }

    #[test]
    fn changement_d_heure_d_automne() {
        // sentry-watch 09:00 Paris : 24/10 (CEST) = 07:00Z ; 25/10 (CET, après le changement) = 08:00Z
        assert_eq!(last_due(utc("2026-10-24T12:00:00Z"), sched("sentry-watch")), Some(utc("2026-10-24T07:00:00Z")));
        assert_eq!(last_due(utc("2026-10-25T12:00:00Z"), sched("sentry-watch")), Some(utc("2026-10-25T08:00:00Z")));
    }

    #[test]
    fn etat_a_jour_en_cours_ou_en_retard() {
        let s = sched("pod-restart-watch"); // grâce 20 min
        let due = utc("2026-10-22T14:00:00Z");
        assert_eq!(evaluate(utc("2026-10-22T14:03:00Z"), s, Some(due)), Health::AJour);
        assert_eq!(evaluate(utc("2026-10-22T14:10:00Z"), s, Some(due - chrono::Duration::hours(1))), Health::EnCours { due_at: due });
        assert_eq!(evaluate(utc("2026-10-22T14:30:00Z"), s, Some(due - chrono::Duration::hours(1))), Health::EnRetard { due_at: due });
        assert_eq!(evaluate(utc("2026-10-22T14:30:00Z"), s, None), Health::EnRetard { due_at: due });
    }

    #[test]
    fn tolerance_de_deux_minutes_avant_l_echeance() {
        let s = sched("pod-restart-watch");
        let due = utc("2026-10-22T14:00:00Z");
        assert_eq!(evaluate(utc("2026-10-22T14:50:00Z"), s, Some(due - chrono::Duration::minutes(1))), Health::AJour);
    }
}
