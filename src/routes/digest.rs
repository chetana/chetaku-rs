//! `GET /digest` : synthèse déterministe (gabarits, sans LLM) de ce qui a bougé sur une période, à partir des seuls rapports reçus.
//! `from` par défaut = minuit à Paris, `to` = maintenant ; le bouton « ce qui a bougé » passe `from` = dernière consultation.

use std::collections::BTreeMap;

use axum::{extract::{Query, State}, http::HeaderMap, Json};
use chrono::{DateTime, TimeZone, Utc};
use chrono_tz::Europe::Paris;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::error::AppError;

const MAX_CHANGES: usize = 200;
const SHOWN_REFS: usize = 5;

#[derive(Debug, sqlx::FromRow)]
pub struct RunLite {
    pub automation: String,
    pub started_at: DateTime<Utc>,
    pub status: String,
    pub headline: String,
    pub items: Value,
    pub impact: Value,
}

pub fn day_start_paris(now: DateTime<Utc>) -> DateTime<Utc> {
    let day = now.with_timezone(&Paris).date_naive().and_hms_opt(0, 0, 0).expect("minuit valide");
    Paris.from_local_datetime(&day).earliest().expect("minuit existe à Paris").with_timezone(&Utc)
}

fn short(r: &str) -> &str {
    r.rfind('!').map_or(r, |i| &r[i..])
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}

fn refs_list(entries: &[(String, String)]) -> String {
    let shown: Vec<String> = entries
        .iter()
        .take(SHOWN_REFS)
        .map(|(r, a)| if a.is_empty() { r.clone() } else { format!("{r} ({a})") })
        .collect();
    let rest = entries.len().saturating_sub(SHOWN_REFS);
    if rest > 0 { format!("{}, et {rest} autres", shown.join(", ")) } else { shown.join(", ") }
}

fn kind_sentence(kind: &str, entries: &[(String, String)], changes: &[&Value]) -> String {
    let n = entries.len();
    match kind {
        "mr" => format!("{} : {}.", plural(n, "MR traitée", "MR traitées"), refs_list(entries)),
        "ticket" => {
            let shown: Vec<(String, String)> = changes
                .iter()
                .filter(|c| s(c, "kind") == "ticket")
                .map(|c| {
                    let (f, t) = (s(c, "from"), s(c, "to"));
                    (s(c, "ref").to_string(), if !f.is_empty() && !t.is_empty() { format!("{f} → {t}") } else { s(c, "action").to_string() })
                })
                .collect();
            format!("{} : {}.", plural(n, "ticket mis à jour", "tickets mis à jour"), refs_list(&shown))
        }
        "thread" => {
            let mut by: BTreeMap<&str, usize> = BTreeMap::new();
            for (_, a) in entries { *by.entry(a.as_str()).or_default() += 1; }
            let detail: Vec<String> = by.iter().map(|(a, c)| format!("{c} {a}")).collect();
            format!("{} ({}).", plural(n, "retour de review traité", "retours de review traités"), detail.join(", "))
        }
        "sentry" => format!("{} : {}.", plural(n, "issue Sentry", "issues Sentry"), refs_list(entries)),
        "pod" => format!("{} du pod prod.", plural(n, "restart", "restarts")),
        "note" => format!("{}.", plural(n, "note de réunion archivée", "notes de réunion archivées")),
        "account" => format!("{} Logto concerné(s).", n),
        "recap" => "Récap de la veille publié.".to_string(),
        other => format!("{} « {other} ».", plural(n, "élément", "éléments")),
    }
}

pub fn build(runs: &[RunLite]) -> Value {
    let mut runs: Vec<&RunLite> = runs.iter().collect();
    runs.sort_by_key(|r| r.started_at);

    let mut by_status = json!({"ok": 0, "noop": 0, "alert": 0, "error": 0});
    for r in &runs {
        if let Some(c) = by_status.get_mut(r.status.as_str()) {
            *c = json!(c.as_u64().unwrap_or(0) + 1);
        }
    }
    let count = |k: &str| by_status[k].as_u64().unwrap_or(0) as usize;
    let (total, active) = (runs.len(), runs.len() - count("noop"));

    let mut changes: Vec<Value> = Vec::new();
    let mut areas: BTreeMap<String, usize> = BTreeMap::new();
    let mut impacts: Vec<Value> = Vec::new();
    for r in &runs {
        for it in r.items.as_array().into_iter().flatten() {
            let mut c = it.clone();
            c["at"] = json!(r.started_at);
            c["automation"] = json!(r.automation);
            changes.push(c);
        }
        for a in r.impact.get("areas").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
            *areas.entry(a.to_string()).or_default() += 1;
        }
        let summary = s(&r.impact, "summary");
        if !summary.is_empty() && !impacts.iter().any(|i| s(i, "summary") == summary) {
            impacts.push(json!({"at": r.started_at, "automation": r.automation, "summary": summary}));
        }
    }
    changes.truncate(MAX_CHANGES);
    let mut areas: Vec<(String, usize)> = areas.into_iter().collect();
    areas.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let areas: Vec<Value> = areas.into_iter().take(10).map(|(a, c)| json!({"area": a, "count": c})).collect();

    let text = if total == 0 {
        "Aucun passage sur la période.".to_string()
    } else {
        let mut parts = vec![format!("{active} avec activité")];
        if count("alert") > 0 { parts.push(plural(count("alert"), "alerte", "alertes")); }
        if count("error") > 0 { parts.push(plural(count("error"), "erreur", "erreurs")); }
        let mut out = vec![if active == 0 {
            format!("{}, aucune activité.", plural(total, "passage", "passages"))
        } else {
            format!("{} ({}).", plural(total, "passage", "passages"), parts.join(", "))
        }];

        let mut by_kind: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
        for c in &changes {
            let list = by_kind.entry(s(c, "kind").to_string()).or_default();
            let r = short(s(c, "ref")).to_string();
            match list.iter_mut().find(|(x, _)| *x == r) {
                Some(e) => e.1 = s(c, "action").to_string(),
                None => list.push((r, s(c, "action").to_string())),
            }
        }
        let order = ["mr", "thread", "ticket", "sentry", "pod", "account", "note", "recap"];
        let mut kinds: Vec<&String> = by_kind.keys().collect();
        kinds.sort_by_key(|k| order.iter().position(|o| o == k).unwrap_or(order.len()));
        for k in kinds {
            let of_kind: Vec<&Value> = changes.iter().filter(|c| s(c, "kind") == k).collect();
            out.push(kind_sentence(k, &by_kind[k], &of_kind));
        }

        let mut latest: BTreeMap<&str, &RunLite> = BTreeMap::new();
        for r in &runs { latest.insert(r.automation.as_str(), r); }
        let watch: Vec<String> = latest
            .values()
            .filter(|r| r.status == "alert" || r.status == "error")
            .map(|r| format!("{} ({} : {})", r.automation, r.status, r.headline))
            .collect();
        if !watch.is_empty() {
            out.push(format!("À regarder : {}.", watch.join(", ")));
        }
        out.join(" ")
    };

    json!({
        "runs": total, "active": active, "by_status": by_status,
        "changes": changes, "areas": areas, "impacts": impacts, "text": text,
    })
}

#[derive(Debug, Deserialize)]
pub struct DigestQuery {
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    automation: Option<String>,
}

pub async fn digest(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Query(q): Query<DigestQuery>,
) -> Result<Json<Value>, AppError> {
    super::runs::guard(&headers).await?;
    let now = Utc::now();
    let (from, to) = (q.from.unwrap_or_else(|| day_start_paris(now)), q.to.unwrap_or(now));
    super::runs::check_range(Some(from), Some(to)).map_err(AppError::Validation)?;
    if q.automation.as_deref().is_some_and(|a| a.is_empty() || a.len() > 64 || !a.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')) {
        return Err(AppError::Validation("automation invalide".into()));
    }
    let rows: Vec<RunLite> = sqlx::query_as(
        "SELECT automation, started_at, status, headline, items, impact FROM automation_runs
         WHERE started_at >= $1 AND started_at < $2 AND ($3::text IS NULL OR automation = $3)
         ORDER BY started_at LIMIT 2000",
    )
    .bind(from)
    .bind(to)
    .bind(&q.automation)
    .fetch_all(&pool)
    .await?;
    let mut d = build(&rows);
    d["from"] = json!(from);
    d["to"] = json!(to);
    Ok(Json(d))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> DateTime<Utc> { s.parse().unwrap() }
    fn run(automation: &str, t: &str, status: &str, headline: &str, items: Value, impact: Value) -> RunLite {
        RunLite { automation: automation.into(), started_at: at(t), status: status.into(), headline: headline.into(), items, impact }
    }
    fn fixture() -> Vec<RunLite> {
        vec![
            run("mr-review-watch", "2026-10-22T09:05:00Z", "ok", "2 MR relues",
                json!([{"kind":"mr","ref":"djustlab/ia/djust-sales!1200","action":"review","title":"Fix matching"},
                       {"kind":"mr","ref":"djustlab/ia/djust-sales!1201","action":"recheck","title":"Autre"}]),
                json!({"summary":"Matching : 1 fil bloquant reste sur !1200","areas":["MR !1200","DAI-1042"]})),
            run("sync-mr-tickets", "2026-10-22T09:20:00Z", "ok", "1 ticket passé en Livré (Dev)",
                json!([{"kind":"ticket","ref":"DAI-1042","action":"transition","from":"Retour recette","to":"Livré (Dev)","title":"MR !1200"}]),
                json!({"summary":"Le matching est livré en dev","areas":["DAI-1042"]})),
            run("pod-restart-watch", "2026-10-22T10:00:00Z", "noop", "RAS", json!([]), json!({})),
            run("sentry-watch", "2026-10-22T07:00:00Z", "alert", "1 nouvelle issue prod",
                json!([{"kind":"sentry","ref":"DJUST-SALES-2A","action":"nouvelle","title":"ValueError"}]), json!({})),
            run("mr-feedback-watch", "2026-10-22T09:30:00Z", "error", "échec (code 3)", json!([]), json!({})),
        ]
    }

    #[test]
    fn compte_les_passages_par_statut() {
        let d = build(&fixture());
        assert_eq!(d["runs"], 5);
        assert_eq!(d["by_status"], json!({"ok":2,"noop":1,"alert":1,"error":1}));
        assert_eq!(d["active"], 4);
    }

    #[test]
    fn changements_chronologiques_et_complets() {
        let d = build(&fixture());
        let c = d["changes"].as_array().unwrap();
        assert_eq!(c.len(), 4);
        assert_eq!(c[0]["automation"], "sentry-watch");
        assert_eq!(c[1]["ref"], "djustlab/ia/djust-sales!1200");
        assert_eq!(c[3]["to"], "Livré (Dev)");
    }

    #[test]
    fn zones_triees_par_frequence_et_impacts_dedoublonnes() {
        let d = build(&fixture());
        assert_eq!(d["areas"][0], json!({"area":"DAI-1042","count":2}));
        assert_eq!(d["impacts"].as_array().unwrap().len(), 2);
        let mut f = fixture();
        f.push(run("mr-review-watch", "2026-10-22T11:05:00Z", "ok", "x", json!([]), json!({"summary":"Le matching est livré en dev","areas":[]})));
        assert_eq!(build(&f)["impacts"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn texte_resume_l_activite_et_ce_qui_demande_attention() {
        let t = build(&fixture())["text"].as_str().unwrap().to_string();
        assert!(t.contains("5 passages"), "{t}");
        assert!(t.contains("1 alerte") && t.contains("1 erreur"), "{t}");
        assert!(t.contains("2 MR") && t.contains("!1200") && t.contains("!1201"), "{t}");
        assert!(t.contains("DAI-1042") && t.contains("Livré (Dev)"), "{t}");
        assert!(t.contains("À regarder"), "{t}");
        assert!(t.contains("sentry-watch") && t.contains("mr-feedback-watch"), "{t}");
        assert!(!t.contains("djustlab/ia"), "les références doivent être raccourcies: {t}");
    }

    #[test]
    fn accord_singulier_pluriel() {
        let f = vec![run("mr-review-watch", "2026-10-22T09:05:00Z", "ok", "h",
            json!([{"kind":"mr","ref":"x/y!7","action":"review"}]), json!({}))];
        let t = build(&f)["text"].as_str().unwrap().to_string();
        assert!(t.contains("1 passage") && !t.contains("1 passages"), "{t}");
        assert!(t.contains("1 MR") && !t.contains("1 MR traitées"), "{t}");
    }

    #[test]
    fn periode_vide_et_periode_sans_activite() {
        let d = build(&[]);
        assert_eq!(d["runs"], 0);
        assert_eq!(d["text"], "Aucun passage sur la période.");
        let calm = vec![run("pod-restart-watch", "2026-10-22T10:00:00Z", "noop", "RAS", json!([]), json!({}))];
        let t = build(&calm)["text"].as_str().unwrap().to_string();
        assert!(t.contains("aucune activité"), "{t}");
        assert!(!t.contains("À regarder"), "{t}");
    }

    #[test]
    fn listes_longues_tronquees() {
        let items: Vec<Value> = (0..12).map(|i| json!({"kind":"mr","ref":format!("x/y!{}", 100 + i),"action":"review"})).collect();
        let f = vec![run("mr-review-watch", "2026-10-22T09:05:00Z", "ok", "h", Value::Array(items), json!({}))];
        let t = build(&f)["text"].as_str().unwrap().to_string();
        assert!(t.contains("12 MR") && t.contains("et 7 autres"), "{t}");
    }

    #[test]
    fn borne_par_defaut_debut_de_journee_a_paris() {
        // 14:30Z le 22/10 (été) = 16:30 Paris → minuit Paris = 21/10 22:00Z
        assert_eq!(day_start_paris(at("2026-10-22T14:30:00Z")), at("2026-10-21T22:00:00Z"));
        // après le changement d'heure : 25/10 12:00Z = 13:00 CET → minuit Paris = 24/10 22:00Z
        assert_eq!(day_start_paris(at("2026-10-25T12:00:00Z")), at("2026-10-24T22:00:00Z"));
    }
}
