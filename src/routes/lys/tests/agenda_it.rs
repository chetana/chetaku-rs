//! T3 : rendez-vous, agenda global, relecture khmère. Postgres jetable (LYS_TEST_DATABASE_URL) pour l'intégration ;
//! l'agenda lui-même est testé en pur avec une horloge fixe.

use super::*;
use crate::error::AppError;
use crate::routes::lys::{build_agenda, parse_starts_at, task_out, AppointmentRow, TaskOut, TaskRow};
use uuid::Uuid;
use chrono::{DateTime, NaiveDate, Utc};
use reqwest::Method as M;


fn at(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

fn day(s: &str) -> NaiveDate {
    s.parse().unwrap()
}

impl Api {
    async fn rdv(&self, task: &str, body: &str) -> (StatusCode, Value) {
        self.call(M::POST, &format!("/lys/tasks/{task}/appointments"), Some("lys2"), Some(body)).await
    }
    async fn annule(&self, id: &str) -> (StatusCode, Value) {
        self.call(M::DELETE, &format!("/lys/appointments/{id}"), Some("lys1"), None).await
    }
    async fn relu(&self, task: &str, body: &str) -> (StatusCode, Value) {
        self.call(M::PUT, &format!("/lys/tasks/{task}/km-reviewed"), Some("lys1"), Some(body)).await
    }
    async fn count_rdv(&self) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM lys_fr.appointment").fetch_one(&self.pool).await.unwrap()
    }
    async fn seed_km(&self) -> String {
        let km = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/migrations/seed/lys_fr_seed_km.sql")).unwrap();
        sqlx::raw_sql(&km).execute(&self.pool).await.unwrap();
        km
    }
    fn find_task<'a>(plan: &'a Value, slug: &str) -> &'a Value {
        plan["categories"].as_array().unwrap().iter().flat_map(|c| c["tasks"].as_array().unwrap()).find(|t| t["slug"] == slug).unwrap()
    }
}

// ── Agenda : règles pures ────────────────────────────────────────────────────

fn task_row(slug: &str, status: &str, due: Option<&str>, over: Option<&str>, legal: Option<&str>) -> TaskOut {
    let row = TaskRow {
        slug: slug.into(),
        category_slug: "c".into(),
        position: 1,
        title_fr: format!("Titre {slug}"),
        title_km: Some(format!("km {slug}")),
        help_fr: None,
        help_km: None,
        status: status.into(),
        note: String::new(),
        due_date: due.map(day),
        due_override: over.map(day),
        legal_deadline: legal.map(day),
        done_at: None,
        km_status: "none".into(),
        km_reviewed_by: None,
        km_reviewed_at: None,
    };
    task_out(row, vec![], vec![], vec![])
}

fn rdv_row(slug: &str, starts: &str) -> AppointmentRow {
    AppointmentRow {
        task_slug: slug.into(),
        id: Uuid::new_v4(),
        starts_at: at(starts),
        place: Some("Mairie".into()),
        note: None,
        created_by: "lys1".into(),
        created_at: at("2026-10-01T00:00:00Z"),
    }
}

#[test]
fn agenda_melange_rdv_et_echeances_tries_avec_jours_restants() {
    let mut a = task_row("a", "todo", Some("2026-11-20"), Some("2026-11-10"), Some("2026-11-30"));
    a.appointments = vec![rdv_row("a", "2026-11-10T08:30:00Z"), rdv_row("a", "2026-10-01T08:30:00Z")];
    let b = task_row("b", "doing", Some("2026-11-05"), None, None);
    let done = task_row("c", "done", Some("2026-11-01"), None, Some("2026-11-02"));
    let now = at("2026-10-27T10:00:00Z");
    let ag = build_agenda(&[a, b, done], now);
    let seq: Vec<(String, String, i64)> = ag
        .iter()
        .map(|e| (e["type"].as_str().unwrap().into(), e["task_slug"].as_str().unwrap().into(), e["days_left"].as_i64().unwrap()))
        .collect();
    assert_eq!(
        seq,
        vec![
            ("due".into(), "b".into(), 9),  // 5 nov : override absent, due_date
            ("due".into(), "a".into(), 14), // 10 nov : due_override l'emporte sur 20 nov
            ("appointment".into(), "a".into(), 14), // même jour : l'échéance avant le rendez-vous
            ("legal".into(), "a".into(), 34),
        ],
        "tâche terminée et rendez-vous passé exclus"
    );
    assert_eq!(ag[0]["date"], "2026-11-05");
    assert_eq!(ag[0]["title_km"], "km b");
    assert!(ag[0]["starts_at"].is_null() && ag[0]["appointment_id"].is_null());
    assert_eq!(ag[2]["place"], "Mairie");
    assert_eq!(ag[2]["starts_at"], "2026-11-10T08:30:00Z");
    assert!(ag[2]["appointment_id"].is_string());
}

#[test]
fn agenda_echeance_depassee_reste_avec_jours_negatifs_et_date_en_heure_de_paris() {
    let late = task_row("late", "blocked", Some("2026-10-20"), None, None);
    let mut t = task_row("t", "todo", None, None, None);
    // 23h30 UTC le 27 octobre = 00h30 le 28 à Paris (UTC+1 après le 25 octobre) : le rendez-vous est « demain ».
    t.appointments = vec![rdv_row("t", "2026-10-27T23:30:00Z")];
    let ag = build_agenda(&[late, t], at("2026-10-27T10:00:00Z"));
    assert_eq!(ag[0]["days_left"], -7);
    assert_eq!(ag[1]["date"], "2026-10-28");
    assert_eq!(ag[1]["days_left"], 1);
    let vide = build_agenda(&[task_row("x", "todo", None, None, None)], at("2026-10-27T10:00:00Z"));
    assert!(vide.is_empty());
}

#[test]
fn bornes_et_formats_de_starts_at() {
    assert!(parse_starts_at("2026-11-09T14:30:00+01:00").is_ok());
    assert!(parse_starts_at("2026-01-01T00:00:00Z").is_ok());
    assert!(parse_starts_at("2030-12-31T23:59:59Z").is_ok());
    assert!(matches!(parse_starts_at("2025-12-31T23:59:59Z"), Err(AppError::Validation(_))));
    assert!(matches!(parse_starts_at("2031-01-01T00:00:00Z"), Err(AppError::Validation(_))));
    assert!(matches!(parse_starts_at("demain"), Err(AppError::Unprocessable(_))));
    assert!(matches!(parse_starts_at("2026-11-09T14:30:00"), Err(AppError::Unprocessable(_))), "fuseau obligatoire");
}

// ── Intégration ──────────────────────────────────────────────────────────────

db_test!(creer_un_rdv_201_trace_et_visible_dans_la_tache_et_le_plan, |a| {
    let (st, v) = a.rdv("cv", r#"{"starts_at":"2029-03-04T09:15:00+01:00","place":"  Mairie de Chessy ","note":"Apporter le livret\nde famille"}"#).await;
    assert_eq!(st, StatusCode::CREATED, "{v}");
    let r = &v["appointment"];
    assert_eq!(r["starts_at"], "2029-03-04T08:15:00Z", "stocké en UTC");
    assert_eq!(r["place"], "Mairie de Chessy", "espaces rognés");
    assert_eq!(r["note"], "Apporter le livret\nde famille");
    assert_eq!(r["created_by"], "lys2");
    assert!(r["id"].is_string() && r["created_at"].is_string());
    assert_eq!(v["task"]["appointments"][0]["id"], r["id"]);
    let p = a.plan().await;
    assert_eq!(Api::find_task(&p, "cv")["appointments"][0]["id"], r["id"]);
    assert_eq!(Api::find_task(&p, "banque-rdv")["appointments"], json!([]), "tableau vide sans rendez-vous");
    // Lieu et note facultatifs : null, et chaînes blanches = absent.
    let (st, v) = a.rdv("cv", r#"{"starts_at":"2029-03-05T10:00:00Z","place":"  ","note":""}"#).await;
    assert_eq!(st, StatusCode::CREATED);
    assert!(v["appointment"]["place"].is_null() && v["appointment"]["note"].is_null());
});

db_test!(rdv_trie_par_date_dans_la_tache_annule_exclu, |a| {
    let (_, tard) = a.rdv("cv", r#"{"starts_at":"2029-05-01T10:00:00Z"}"#).await;
    let (_, tot) = a.rdv("cv", r#"{"starts_at":"2029-04-01T10:00:00Z"}"#).await;
    let ids: Vec<Value> = Api::find_task(&a.plan().await, "cv")["appointments"].as_array().unwrap().iter().map(|x| x["id"].clone()).collect();
    assert_eq!(ids, vec![tot["appointment"]["id"].clone(), tard["appointment"]["id"].clone()]);
    let (st, _) = a.annule(tot["appointment"]["id"].as_str().unwrap()).await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    let p = a.plan().await;
    let ids: Vec<Value> = Api::find_task(&p, "cv")["appointments"].as_array().unwrap().iter().map(|x| x["id"].clone()).collect();
    assert_eq!(ids, vec![tard["appointment"]["id"].clone()]);
    assert!(p["agenda"].as_array().unwrap().iter().all(|e| e["appointment_id"] != tot["appointment"]["id"]));
});

db_test!(annulation_douce_idempotente_404_400, |a| {
    let (_, v) = a.rdv("cv", r#"{"starts_at":"2029-04-01T10:00:00Z"}"#).await;
    let id = v["appointment"]["id"].as_str().unwrap();
    assert_eq!(a.annule(id).await.0, StatusCode::NO_CONTENT);
    let first: Option<DateTime<Utc>> = sqlx::query_scalar("SELECT cancelled_at FROM lys_fr.appointment WHERE id = $1::uuid").bind(id).fetch_one(&a.pool).await.unwrap();
    assert!(first.is_some());
    assert_eq!(a.annule(id).await.0, StatusCode::NO_CONTENT, "idempotent");
    let again: Option<DateTime<Utc>> = sqlx::query_scalar("SELECT cancelled_at FROM lys_fr.appointment WHERE id = $1::uuid").bind(id).fetch_one(&a.pool).await.unwrap();
    assert_eq!(first, again, "horodatage d'origine conservé");
    assert_eq!(a.count_rdv().await, 1, "ligne conservée (suppression douce)");
    assert_eq!(a.annule("123e4567-e89b-42d3-a456-426614174000").await.0, StatusCode::NOT_FOUND);
    assert_eq!(a.annule("pas-un-uuid").await.0, StatusCode::BAD_REQUEST);
});

db_test!(rdv_refus_400_422_404_sans_ecriture, |a| {
    let long_place = "x".repeat(201);
    let long_note = "x".repeat(501);
    let cas_400 = [
        r#"{"starts_at":"2025-12-31T23:59:59Z"}"#.to_string(),
        r#"{"starts_at":"2031-01-01T00:00:00Z"}"#.to_string(),
        r#"{"starts_at":"0026-11-09T10:00:00Z"}"#.to_string(),
        format!(r#"{{"starts_at":"2029-01-01T10:00:00Z","place":"{long_place}"}}"#),
        format!(r#"{{"starts_at":"2029-01-01T10:00:00Z","note":"{long_note}"}}"#),
        r#"{"starts_at":"2029-01-01T10:00:00Z","note":"a\u0000b"}"#.to_string(),
        r#"{"starts_at":"2029-01-01T10:00:00Z","place":"a\nb"}"#.to_string(),
        r#"{"starts_at":"#.to_string(),
    ];
    for c in &cas_400 {
        assert_eq!(a.rdv("cv", c).await.0, StatusCode::BAD_REQUEST, "{c}");
    }
    let cas_422 = [
        r#"{"starts_at":"demain"}"#,
        r#"{"starts_at":"2029-01-01T10:00:00"}"#,
        r#"{}"#,
        r#"{"starts_at":"2029-01-01T10:00:00Z","titre":"x"}"#,
        r#"{"starts_at":12}"#,
        r#"{"starts_at":"2029-01-01T10:00:00Z","place":3}"#,
    ];
    for c in cas_422 {
        assert_eq!(a.rdv("cv", c).await.0, StatusCode::UNPROCESSABLE_ENTITY, "{c}");
    }
    assert_eq!(a.rdv("inconnue", r#"{"starts_at":"2029-01-01T10:00:00Z"}"#).await.0, StatusCode::NOT_FOUND);
    assert_eq!(a.count_rdv().await, 0, "aucun refus n'a écrit");
    let note_500 = "é".repeat(500);
    let (st, _) = a.rdv("cv", &format!(r#"{{"starts_at":"2029-01-01T10:00:00Z","note":"{note_500}"}}"#)).await;
    assert_eq!(st, StatusCode::CREATED, "500 caractères (pas octets) acceptés");
});

db_test!(agenda_global_melange_rdv_et_echeances_trie, |a| {
    a.patch("cv", r#"{"due_override":"2029-06-10"}"#).await;
    a.patch("mutuelle", r#"{"due_override":"2029-06-20"}"#).await;
    sqlx::query("UPDATE lys_fr.task SET legal_deadline = '2029-06-15' WHERE slug = 'vls-ts-anef'").execute(&a.pool).await.unwrap();
    a.rdv("cv", r#"{"starts_at":"2029-06-12T10:00:00Z","place":"Mairie"}"#).await;
    a.rdv("banque-rdv", r#"{"starts_at":"2029-06-01T08:00:00Z"}"#).await;
    a.rdv("cv", r#"{"starts_at":"2026-02-01T08:00:00Z"}"#).await; // passé : dans la tâche, pas dans l'agenda
    a.patch("mutuelle", r#"{"status":"done"}"#).await;
    let p = a.plan().await;
    let ag = p["agenda"].as_array().unwrap();
    let dates: Vec<&str> = ag.iter().map(|e| e["date"].as_str().unwrap()).collect();
    let mut sorted = dates.clone();
    sorted.sort();
    assert_eq!(dates, sorted, "trié par date");
    let pick = |ty: &str, slug: &str| ag.iter().position(|e| e["type"] == ty && e["task_slug"] == slug);
    let (rdv_banque, due_cv, rdv_cv, legal) = (
        pick("appointment", "banque-rdv").unwrap(),
        pick("due", "cv").unwrap(),
        pick("appointment", "cv").unwrap(),
        pick("legal", "vls-ts-anef").unwrap(),
    );
    assert!(rdv_banque < due_cv && due_cv < rdv_cv && rdv_cv < legal, "{dates:?}");
    assert!(pick("due", "mutuelle").is_none(), "tâche terminée absente");
    assert_eq!(ag.iter().filter(|e| e["type"] == "appointment").count(), 2, "rendez-vous passé exclu");
    let e = &ag[rdv_cv];
    assert_eq!((e["place"].as_str(), e["title_fr"].as_str().is_some(), e["days_left"].is_i64()), (Some("Mairie"), true, true));
    // Une échéance seed non terminée est bien là (due_date) et distinguée du légal.
    assert!(ag.iter().any(|e| e["type"] == "due" && e["task_slug"] == "vls-ts-anef"));
});

db_test!(plan_retro_compatible_anciens_champs_inchanges, |a| {
    let p = a.plan().await;
    for k in ["totals", "next", "categories"] {
        assert!(!p[k].is_null() || k == "next", "{k}");
    }
    let t = Api::find_task(&p, "vls-ts-anef");
    for k in [
        "slug", "category_slug", "position", "title_fr", "title_km", "help_fr", "help_km", "status", "note", "due_date",
        "due_override", "legal_deadline", "effective_due", "done_at", "km_status", "items_done", "items_total", "items", "documents",
    ] {
        assert!(t.get(k).is_some(), "champ historique manquant : {k}");
    }
    assert_eq!(p["totals"]["items_done"], 0);
    assert!(p["agenda"].is_array());
    // La réponse des routes existantes porte aussi les rendez-vous de la tâche.
    let (_, v) = a.done("cv.1", true, "lys1").await;
    assert_eq!(v["task"]["appointments"], json!([]));
});

db_test!(relu_pose_statut_qui_quand_puis_retour_a_draft, |a| {
    a.seed_km().await;
    let (st, v) = a.relu("cv", r#"{"reviewed":true}"#).await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(v["task"]["km_status"], "reviewed");
    assert_eq!(v["task"]["km_reviewed_by"], "lys1");
    assert!(v["task"]["km_reviewed_at"].is_string());
    let t = Api::find_task(&a.plan().await, "cv").clone();
    assert_eq!(t["km_status"], "reviewed");
    let it: i64 = sqlx::query_scalar("SELECT count(*) FROM lys_fr.item WHERE task_slug='cv' AND km_status='draft'").fetch_one(&a.pool).await.unwrap();
    assert_eq!(it, 0, "les textes des cases passent aussi reviewed");
    // Idempotent.
    assert_eq!(a.relu("cv", r#"{"reviewed":true}"#).await.0, StatusCode::OK);
    let (st, v) = a.relu("cv", r#"{"reviewed":false}"#).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["task"]["km_status"], "draft");
    assert!(v["task"]["km_reviewed_by"].is_null() && v["task"]["km_reviewed_at"].is_null());
    // Autre tâche intacte.
    assert_ne!(Api::find_task(&a.plan().await, "mutuelle")["km_status"], "reviewed");
});

db_test!(relu_refus_422_404_et_tache_sans_khmer, |a| {
    assert_eq!(a.relu("cv", r#"{"reviewed":"oui"}"#).await.0, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(a.relu("cv", r#"{}"#).await.0, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(a.relu("cv", r#"{"reviewed":true,"x":1}"#).await.0, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(a.relu("cv", r#"{"reviewed":"#).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(a.relu("inconnue", r#"{"reviewed":true}"#).await.0, StatusCode::NOT_FOUND);
    sqlx::query("UPDATE lys_fr.task SET km_status='none' WHERE slug='cv'").execute(&a.pool).await.unwrap();
    assert_eq!(a.relu("cv", r#"{"reviewed":true}"#).await.0, StatusCode::BAD_REQUEST, "rien à relire");
});

db_test!(seed_km_rejoue_ne_touche_pas_un_reviewed, |a| {
    let km = a.seed_km().await;
    let (t_before,): (Option<String>,) = sqlx::query_as("SELECT title_km FROM lys_fr.task WHERE slug='cv'").fetch_one(&a.pool).await.unwrap();
    assert!(t_before.is_some(), "le seed KM a bien posé le khmer");
    a.relu("cv", r#"{"reviewed":true}"#).await;
    // Lys corrige le texte, puis le seed est rejoué (déploiement) : sa version relue doit survivre.
    sqlx::query("UPDATE lys_fr.task SET title_km = 'corrigé par Lys' WHERE slug='cv'").execute(&a.pool).await.unwrap();
    sqlx::query("UPDATE lys_fr.item SET label_km = 'case corrigée' WHERE slug='cv.1'").execute(&a.pool).await.unwrap();
    sqlx::raw_sql(&km).execute(&a.pool).await.unwrap();
    let (title, status, by): (String, String, Option<String>) =
        sqlx::query_as("SELECT title_km, km_status, km_reviewed_by FROM lys_fr.task WHERE slug='cv'").fetch_one(&a.pool).await.unwrap();
    assert_eq!((title.as_str(), status.as_str(), by.as_deref()), ("corrigé par Lys", "reviewed", Some("lys1")));
    let (label, st): (String, String) = sqlx::query_as("SELECT label_km, km_status FROM lys_fr.item WHERE slug='cv.1'").fetch_one(&a.pool).await.unwrap();
    assert_eq!((label.as_str(), st.as_str()), ("case corrigée", "reviewed"));
    // Une tâche non relue est bien rejouée (draft).
    let m: String = sqlx::query_scalar("SELECT km_status FROM lys_fr.task WHERE slug='mutuelle'").fetch_one(&a.pool).await.unwrap();
    assert_eq!(m, "draft");
});

db_test!(migration_008_rejouable_sans_perte, |a| {
    a.seed_km().await;
    a.rdv("cv", r#"{"starts_at":"2029-04-01T10:00:00Z"}"#).await;
    a.relu("cv", r#"{"reviewed":true}"#).await;
    let sql = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/migrations/008_lys_agenda.sql")).unwrap();
    sqlx::raw_sql(&sql).execute(&a.pool).await.unwrap();
    sqlx::raw_sql(&sql).execute(&a.pool).await.unwrap();
    assert_eq!(a.count_rdv().await, 1);
    let by: Option<String> = sqlx::query_scalar("SELECT km_reviewed_by FROM lys_fr.task WHERE slug='cv'").fetch_one(&a.pool).await.unwrap();
    assert_eq!(by.as_deref(), Some("lys1"));
});
