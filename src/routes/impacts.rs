//! `GET /impacts` : historique des changements portés par les automatisations, par jour, semaine (lundi) ou mois, en heure de Paris.
//! Une simple vérification n'est pas un changement : elle n'apparaît qu'avec `scope=all`.

use std::collections::BTreeMap;

use axum::{extract::State, http::HeaderMap, Json};
use chrono::{DateTime, Datelike, Duration, Utc};
use chrono_tz::Europe::Paris;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::error::AppError;
use crate::routes::runs::guard;

const OBSERVATIONS: [&str; 5] = ["recheck", "attend", "reserve", "a_avancer", "resume"];
const MAX_RUNS: i64 = 5000;
const MAX_EVENTS_PER_BUCKET: usize = 200;
const TOP_AREAS: usize = 15;

#[derive(Debug, Clone)]
pub struct RunLite {
    pub id: i64,
    pub automation: String,
    pub started_at: DateTime<Utc>,
    pub items: Value,
    pub impact: Value,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Group { Day, Week, Month }

impl Group {
    pub fn parse(s: &str) -> Result<Group, String> {
        match s {
            "day" => Ok(Group::Day),
            "week" => Ok(Group::Week),
            "month" => Ok(Group::Month),
            _ => Err("group invalide (day|week|month)".into()),
        }
    }
    fn name(self) -> &'static str {
        match self { Group::Day => "day", Group::Week => "week", Group::Month => "month" }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Scope { Changes, All }

impl Scope {
    pub fn parse(s: &str) -> Result<Scope, String> {
        match s {
            "changes" => Ok(Scope::Changes),
            "all" => Ok(Scope::All),
            _ => Err("scope invalide (changes|all)".into()),
        }
    }
}

pub fn is_change(action: &str) -> bool {
    !action.is_empty() && !OBSERVATIONS.contains(&action)
}

/// Début de la tranche, en heure de Paris, au format `AAAA-MM-JJTHH:MM:SS` (celui de `/runs/summary`).
pub fn bucket_start(t: DateTime<Utc>, g: Group) -> String {
    let d = t.with_timezone(&Paris).date_naive();
    let start = match g {
        Group::Day => d,
        Group::Week => d - Duration::days(i64::from(d.weekday().num_days_from_monday())),
        Group::Month => d.with_day(1).unwrap_or(d),
    };
    format!("{start}T00:00:00")
}

fn text<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}

/// Nom court de ce qui a changé : « MR !1200 » pour une MR, la référence telle quelle sinon (ticket, fil).
pub fn area_of(kind: &str, reference: &str) -> String {
    if kind == "mr" {
        if let Some((_, iid)) = reference.rsplit_once('!') {
            return format!("MR !{iid}");
        }
    }
    reference.to_string()
}

fn clip(s: &str, max: usize) -> String {
    s.chars().filter(|c| !c.is_control()).take(max).collect()
}

struct Event {
    at: DateTime<Utc>,
    value: Value,
    area: String,
}

pub fn build(runs: &[RunLite], group: Group, scope: Scope, area: Option<&str>) -> Value {
    // tranche -> clé de dédoublonnage -> événement ; la même vérification répétée à chaque passage ne compte qu'une fois par tranche.
    let mut buckets: BTreeMap<String, BTreeMap<String, (Event, i64)>> = BTreeMap::new();
    let mut run_ids: BTreeMap<String, std::collections::BTreeSet<i64>> = BTreeMap::new();

    for r in runs {
        let summary = clip(text(&r.impact, "summary"), 300);
        for it in r.items.as_array().into_iter().flatten() {
            let action = text(it, "action");
            if scope == Scope::Changes && !is_change(action) {
                continue;
            }
            let (kind, reference) = (text(it, "kind"), text(it, "ref"));
            let a = area_of(kind, reference);
            if area.is_some_and(|wanted| wanted != a) {
                continue;
            }
            let bucket = bucket_start(r.started_at, group);
            let key = format!("{kind}|{reference}|{action}|{}|{}", text(it, "from"), text(it, "to"));
            run_ids.entry(bucket.clone()).or_default().insert(r.id);
            let slot = buckets.entry(bucket).or_default().entry(key).or_insert_with(|| {
                let value = json!({
                    "at": r.started_at, "run_id": r.id, "automation": r.automation,
                    "kind": kind, "ref": reference, "area": a, "action": action,
                    "title": clip(text(it, "title"), 160), "url": text(it, "url"),
                    "from": clip(text(it, "from"), 80), "to": clip(text(it, "to"), 80),
                    "summary": summary,
                });
                (Event { at: r.started_at, value, area: a.clone() }, 0)
            });
            slot.1 += 1;
            if r.started_at < slot.0.at {
                slot.0.at = r.started_at;
                slot.0.value["at"] = json!(r.started_at);
                slot.0.value["run_id"] = json!(r.id);
            }
        }
    }

    let mut areas: BTreeMap<String, i64> = BTreeMap::new();
    let mut out: Vec<Value> = Vec::new();
    let mut total = 0;
    for (bucket, events) in buckets.iter().rev() {
        let mut list: Vec<(&Event, i64)> = events.values().map(|(e, n)| (e, *n)).collect();
        list.sort_by(|a, b| b.0.at.cmp(&a.0.at));
        for (e, _) in &list {
            *areas.entry(e.area.clone()).or_default() += 1;
        }
        total += list.len() as i64;
        let shown: Vec<Value> = list
            .iter()
            .take(MAX_EVENTS_PER_BUCKET)
            .map(|(e, n)| {
                let mut v = e.value.clone();
                v["times"] = json!(n);
                v
            })
            .collect();
        out.push(json!({
            "bucket": bucket,
            "changes": list.len(),
            "runs": run_ids.get(bucket).map_or(0, |s| s.len()),
            "events": shown,
        }));
    }

    let mut ranked: Vec<(String, i64)> = areas.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let areas: Vec<Value> = ranked.into_iter().take(TOP_AREAS).map(|(a, c)| json!({ "area": a, "count": c })).collect();

    json!({ "group": group.name(), "tz": "Europe/Paris", "scope": if scope == Scope::All { "all" } else { "changes" },
            "area": area, "total": total, "areas": areas, "buckets": out })
}

#[derive(Debug, Deserialize)]
pub struct ImpactsQuery {
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    group: Option<String>,
    scope: Option<String>,
    area: Option<String>,
}

fn valid_area(a: &str) -> bool {
    !a.is_empty() && a.chars().count() <= 80 && a.chars().all(|c| !c.is_control())
}

pub async fn list(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    axum::extract::Query(q): axum::extract::Query<ImpactsQuery>,
) -> Result<Json<Value>, AppError> {
    guard(&headers).await?;
    let group = Group::parse(q.group.as_deref().unwrap_or("day")).map_err(AppError::Validation)?;
    let scope = Scope::parse(q.scope.as_deref().unwrap_or("changes")).map_err(AppError::Validation)?;
    if q.area.as_deref().is_some_and(|a| !valid_area(a)) {
        return Err(AppError::Validation("area invalide".into()));
    }
    crate::routes::runs::check_range(q.from, q.to).map_err(AppError::Validation)?;
    let to = q.to.unwrap_or_else(Utc::now);
    let from = q.from.unwrap_or(to - Duration::days(30));

    let rows: Vec<(i64, String, DateTime<Utc>, Value, Value)> = sqlx::query_as(
        "SELECT id, automation, started_at, items, impact FROM automation_runs
         WHERE started_at >= $1 AND started_at < $2 AND jsonb_array_length(items) > 0
         ORDER BY started_at DESC LIMIT $3",
    )
    .bind(from)
    .bind(to)
    .bind(MAX_RUNS)
    .fetch_all(&pool)
    .await?;
    let runs: Vec<RunLite> = rows
        .into_iter()
        .map(|(id, automation, started_at, items, impact)| RunLite { id, automation, started_at, items, impact })
        .collect();
    Ok(Json(build(&runs, group, scope, q.area.as_deref())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> DateTime<Utc> { s.parse().unwrap() }

    fn run(id: i64, automation: &str, t: &str, items: Value, summary: &str) -> RunLite {
        RunLite { id, automation: automation.into(), started_at: at(t), items, impact: json!({ "summary": summary, "areas": [] }) }
    }

    fn item(kind: &str, r: &str, action: &str) -> Value { json!({ "kind": kind, "ref": r, "action": action }) }

    fn fixture() -> Vec<RunLite> {
        vec![
            run(1, "mr-feedback-watch", "2026-10-22T08:00:00Z", json!([item("thread", "t1", "corrige"), item("mr", "x/y!1200", "recheck")]), "Fil corrigé"),
            run(2, "mr-review-watch", "2026-10-22T09:05:00Z", json!([item("mr", "x/y!1200", "recheck"), item("mr", "x/y!1201", "review")]), "Relue"),
            run(3, "sync-mr-tickets", "2026-10-23T10:20:00Z", json!([{ "kind": "ticket", "ref": "DAI-1042", "action": "transition", "from": "Retour recette", "to": "Livré (Dev)" }]), "Livré en dev"),
            run(4, "mr-review-watch", "2026-10-23T11:05:00Z", json!([item("mr", "x/y!1201", "review")]), "Relue encore"),
        ]
    }

    #[test]
    fn les_verifications_ne_sont_pas_des_changements() {
        for a in ["recheck", "attend", "reserve", "a_avancer", "resume", ""] {
            assert!(!is_change(a), "{a}");
        }
        for a in ["corrige", "rebase", "close", "review", "transition", "rapport", "question", "action_inconnue"] {
            assert!(is_change(a), "{a}");
        }
    }

    #[test]
    fn par_defaut_seuls_les_changements_comptent() {
        let v = build(&fixture(), Group::Day, Scope::Changes, None);
        assert_eq!(v["total"], 4);
        let buckets = v["buckets"].as_array().unwrap();
        assert_eq!(buckets.len(), 2);
        assert_eq!(buckets[0]["bucket"], "2026-10-23T00:00:00");
        assert_eq!(buckets[1]["bucket"], "2026-10-22T00:00:00");
    }

    #[test]
    fn scope_all_ajoute_les_verifications() {
        let v = build(&fixture(), Group::Day, Scope::All, None);
        assert_eq!(v["total"], 5);
    }

    #[test]
    fn la_meme_action_repetee_dans_une_tranche_compte_une_fois() {
        let runs = vec![
            run(1, "a", "2026-10-23T08:00:00Z", json!([item("mr", "x/y!9", "review")]), ""),
            run(2, "a", "2026-10-23T09:00:00Z", json!([item("mr", "x/y!9", "review")]), ""),
        ];
        let v = build(&runs, Group::Day, Scope::Changes, None);
        let b = &v["buckets"][0];
        assert_eq!(b["changes"], 1);
        assert_eq!(b["runs"], 2);
        assert_eq!(b["events"][0]["times"], 2);
        assert_eq!(b["events"][0]["run_id"], 1);
    }

    #[test]
    fn la_meme_action_sur_deux_jours_compte_deux_fois() {
        let runs = vec![
            run(1, "a", "2026-10-22T08:00:00Z", json!([item("mr", "x/y!9", "rebase")]), ""),
            run(2, "a", "2026-10-23T08:00:00Z", json!([item("mr", "x/y!9", "rebase")]), ""),
        ];
        assert_eq!(build(&runs, Group::Day, Scope::Changes, None)["total"], 2);
        assert_eq!(build(&runs, Group::Week, Scope::Changes, None)["total"], 1);
    }

    #[test]
    fn la_semaine_commence_le_lundi_a_paris() {
        // 2026-10-25 est un dimanche ; 23:30 UTC y est déjà lundi 26 à Paris (heure d'hiver).
        assert_eq!(bucket_start(at("2026-10-25T21:30:00Z"), Group::Week), "2026-10-19T00:00:00");
        assert_eq!(bucket_start(at("2026-10-25T23:30:00Z"), Group::Week), "2026-10-26T00:00:00");
        assert_eq!(bucket_start(at("2026-10-26T10:00:00Z"), Group::Week), "2026-10-26T00:00:00");
    }

    #[test]
    fn le_jour_et_le_mois_suivent_l_heure_de_paris() {
        assert_eq!(bucket_start(at("2026-10-22T22:30:00Z"), Group::Day), "2026-10-23T00:00:00");
        assert_eq!(bucket_start(at("2026-10-31T23:30:00Z"), Group::Month), "2026-11-01T00:00:00");
        assert_eq!(bucket_start(at("2026-10-15T10:00:00Z"), Group::Month), "2026-10-01T00:00:00");
    }

    #[test]
    fn nom_court_des_mr_et_des_tickets() {
        assert_eq!(area_of("mr", "djustlab/ia/djust-sales!1200"), "MR !1200");
        assert_eq!(area_of("ticket", "DAI-1042"), "DAI-1042");
        assert_eq!(area_of("mr", "sans-point-exclamation"), "sans-point-exclamation");
    }

    #[test]
    fn filtre_par_aire_pour_suivre_une_mr_dans_le_temps() {
        let v = build(&fixture(), Group::Day, Scope::All, Some("MR !1201"));
        assert_eq!(v["total"], 2);
        assert!(v["buckets"].as_array().unwrap().iter().all(|b| b["events"].as_array().unwrap().iter().all(|e| e["area"] == "MR !1201")));
        assert_eq!(v["area"], "MR !1201");
    }

    #[test]
    fn aires_classees_par_frequence() {
        let v = build(&fixture(), Group::Day, Scope::All, None);
        let areas = v["areas"].as_array().unwrap();
        assert_eq!(areas[0], json!({ "area": "MR !1201", "count": 2 }));
        assert!(areas.iter().any(|a| a["area"] == "DAI-1042"));
    }

    #[test]
    fn transition_de_ticket_gardee_avec_avant_apres_et_resume() {
        let v = build(&fixture(), Group::Day, Scope::Changes, Some("DAI-1042"));
        let e = &v["buckets"][0]["events"][0];
        assert_eq!((e["from"].as_str(), e["to"].as_str()), (Some("Retour recette"), Some("Livré (Dev)")));
        assert_eq!(e["summary"], "Livré en dev");
    }

    #[test]
    fn aucun_passage_aucun_resultat() {
        let v = build(&[], Group::Week, Scope::Changes, None);
        assert_eq!((v["total"].as_i64(), v["buckets"].as_array().map(Vec::len)), (Some(0), Some(0)));
    }

    #[test]
    fn controle_des_parametres() {
        assert!(Group::parse("hour").is_err());
        assert!(Scope::parse("tout").is_err());
        assert!(valid_area("MR !12") && !valid_area("") && !valid_area(&"x".repeat(81)) && !valid_area("a\nb"));
    }
}
