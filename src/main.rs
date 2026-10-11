mod analytics;
mod auth;
mod db;
mod exam;
mod views;

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect};
use axum::routing::{get, post};
use axum::{Form, Json, Router};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::str::FromStr;
use axum::routing::get_service;
use tower_http::services::ServeDir;

use analytics::{analyse, coach_notes};
use auth::Ctx;
use exam::{BuildForm, SaveIn};

type CtxArc = Arc<Ctx>;

fn hx(body: String, code: StatusCode) -> impl IntoResponse {
    (code, Html(body))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_max_level(tracing::Level::DEBUG).init();
    let db_path = std::env::var("PYQ_DB").unwrap_or_else(|_| "pyq.db".into());
    let base = std::env::var("PYQ_BASE").unwrap_or_else(|_| String::new());
    let addr = std::env::var("PYQ_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let opts = SqliteConnectOptions::from_str(&format!("sqlite://{db_path}?mode=rwc"))?
        .create_if_missing(true)
        .busy_timeout(std::time::Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(opts)
        .await?;
    sqlx::query("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;").execute(&pool).await?;
    db::init_db(&pool).await?;
    let ctx = Arc::new(Ctx { pool, base: base.clone() });
    let static_dir = format!("{}/static", std::env::current_dir()?.display());

    let app = Router::new()
        .route("/", get(home))
        .route("/student", get(student_get).post(student_post))
        .route("/logout", get(auth::logout))
        .route("/terms", get(terms))
        .route("/privacy", get(privacy))
        .route("/build", get(build_page).post(build_post))
        .route("/attempt/{paper}", get(attempt_page).post(start_attempt))
        .route("/attempt/{attempt}/answer", post(save_answers))
        .route("/attempt/{attempt}/submit", post(submit))
        .route("/result/{attempt}", get(result_page))
        .route("/history", get(history_page))
        .route("/dashboard", get(dashboard_page))
        .route("/s/{token}", get(shared_paper))
        .route("/api/attempt/{attempt}/state", get(attempt_state))
        .route("/api/attempt/{attempt}/submit", post(submit_api))
        .nest_service("/static", get_service(ServeDir::new(static_dir)))
        .with_state(ctx);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("pyq clone listening on {addr} (base '{base}')");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn home(State(ctx): State<CtxArc>) -> impl IntoResponse {
    let c: (i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM questions),(SELECT COUNT(DISTINCT subject||'-'||year||'-'||shift) FROM questions),(SELECT COUNT(*) FROM subtopics),(SELECT COUNT(DISTINCT subject) FROM questions),(SELECT COUNT(*) FROM users)",
    )
    .fetch_one(&ctx.pool)
    .await
    .unwrap_or((0, 0, 0, 0, 0));
    hx(views::landing(&ctx, c.0, c.1, c.2, c.3, c.4, false), StatusCode::OK)
}

async fn terms(State(ctx): State<CtxArc>) -> impl IntoResponse {
    hx(views::legal(&ctx, "Terms of use", TERMS), StatusCode::OK)
}

async fn privacy(State(ctx): State<CtxArc>) -> impl IntoResponse {
    hx(views::legal(&ctx, "Privacy policy", PRIVACY), StatusCode::OK)
}

async fn student_get(State(ctx): State<CtxArc>, h: HeaderMap, q: Query<std::collections::HashMap<String, String>>) -> axum::response::Response {
    let next = q.get("next").cloned().unwrap_or_else(|| "/build".into());
    if auth::user_id(&ctx.pool, &h).await.is_some() {
        return Redirect::to(&format!("{}{}", ctx.base, next)).into_response();
    }
    hx(views::student(&ctx, &next, None), StatusCode::OK).into_response()
}

async fn student_post(State(ctx): State<CtxArc>, Form(f): Form<auth::StudentForm>) -> axum::response::Response {
    auth::student_post(State(ctx), axum::Form(f)).await
}

async fn build_page(State(ctx): State<CtxArc>, h: HeaderMap) -> axum::response::Response {
    let (_uid, name) = match ctx.require_user(&h).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT c.id, c.subject, c.name, s.name AS sub, (SELECT COUNT(*) FROM questions q WHERE q.chapter_id=c.id) AS n
         FROM chapters c JOIN subtopics s ON s.chapter_id=c.id ORDER BY c.subject, c.name, s.name",
    )
    .fetch_all(&ctx.pool)
    .await
    .unwrap_or_default();
    let mut blocks: Vec<views::SubjectBlock> = Vec::new();
    for r in &rows {
        let subject: String = r.get("subject");
        let id: i64 = r.get("id");
        let cname: String = r.get("name");
        let sub: String = r.get("sub");
        let n: i64 = r.get("n");
        if !blocks.iter().any(|b| b.subject == subject) {
            blocks.push(views::SubjectBlock { subject: subject.clone(), chapters: Vec::new() });
        }
        let blk = blocks.iter_mut().find(|b| b.subject == subject).unwrap();
        match blk.chapters.iter_mut().find(|c| c.id == id) {
            Some(c) => {
                c.subtopics.push(sub);
                c.count = n;
            }
            None => blk.chapters.push(views::ChapterBlock { id, name: cname, subtopics: vec![sub], count: n }),
        }
    }
    let available: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM questions").fetch_one(&ctx.pool).await.unwrap_or(0);
    let form = BuildForm { chapters: String::new(), count: Some(12), duration: Some(24), unseen_only: Some("on".into()) };
    hx(views::build(&ctx, &name, blocks, &form, available, None), StatusCode::OK).into_response()
}

async fn build_post(State(ctx): State<CtxArc>, h: HeaderMap, Form(f): Form<BuildForm>) -> axum::response::Response {
    let (uid, _name) = match ctx.require_user(&h).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    match exam::build_paper(&ctx.pool, &uid, &f).await {
        Ok((paper, _share)) => Redirect::to(&format!("{}/attempt/{paper}", ctx.base)).into_response(),
        Err(e) => {
            use sqlx::Row;
            let rows = sqlx::query(
                "SELECT c.id, c.subject, c.name, s.name AS sub, (SELECT COUNT(*) FROM questions q WHERE q.chapter_id=c.id) AS n
                 FROM chapters c JOIN subtopics s ON s.chapter_id=c.id ORDER BY c.subject, c.name, s.name",
            )
            .fetch_all(&ctx.pool)
            .await
            .unwrap_or_default();
            let mut blocks: Vec<views::SubjectBlock> = Vec::new();
            for r in &rows {
                let subject: String = r.get("subject");
                let id: i64 = r.get("id");
                let cname: String = r.get("name");
                let sub: String = r.get("sub");
                let n: i64 = r.get("n");
                if !blocks.iter().any(|b| b.subject == subject) {
                    blocks.push(views::SubjectBlock { subject: subject.clone(), chapters: Vec::new() });
                }
                let blk = blocks.iter_mut().find(|b| b.subject == subject).unwrap();
                match blk.chapters.iter_mut().find(|c| c.id == id) {
                    Some(c) => {
                        c.subtopics.push(sub);
                        c.count = n;
                    }
                    None => blk.chapters.push(views::ChapterBlock { id, name: cname, subtopics: vec![sub], count: n }),
                }
            }
            let available: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM questions").fetch_one(&ctx.pool).await.unwrap_or(0);
            let msg = e.to_string();
            hx(views::build(&ctx, &_name, blocks, &f, available, Some(&msg)), StatusCode::OK).into_response()
        }
    }
}

async fn start_attempt(State(ctx): State<CtxArc>, h: HeaderMap, Path(paper): Path<String>) -> axum::response::Response {
    let (uid, _) = match ctx.require_user(&h).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM papers WHERE id=? AND user_id=?")
        .bind(&paper)
        .bind(&uid)
        .fetch_one(&ctx.pool)
        .await
        .unwrap_or(0);
    if exists == 0 {
        return hx(views::error(&ctx, 404, ""), StatusCode::NOT_FOUND).into_response();
    }
    match exam::start_attempt(&ctx.pool, &uid, &paper).await {
        Ok(a) => Redirect::to(&format!("{}/attempt/{paper}?run={a}", ctx.base)).into_response(),
        Err(e) => {
            let m = e.to_string();
            hx(views::error(&ctx, 500, &m), StatusCode::INTERNAL_SERVER_ERROR).into_response()
        }
    }
}

async fn attempt_page(State(ctx): State<CtxArc>, h: HeaderMap, Path(paper): Path<String>, q: Query<std::collections::HashMap<String, String>>) -> axum::response::Response {
    let (uid, name) = match ctx.require_user(&h).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let attempt_id = match q.get("run") {
        Some(a) => a.clone(),
        None => match exam::start_attempt(&ctx.pool, &uid, &paper).await {
            Ok(a) => a,
            Err(_) => return hx(views::error(&ctx, 404, ""), StatusCode::NOT_FOUND).into_response(),
        },
    };
    let owned: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM attempts WHERE id=? AND user_id=? AND paper_id=?")
        .bind(&attempt_id)
        .bind(&uid)
        .bind(&paper)
        .fetch_one(&ctx.pool)
        .await
        .unwrap_or(0);
    if owned == 0 {
        return hx(views::error(&ctx, 404, ""), StatusCode::NOT_FOUND).into_response();
    }
    let questions = match exam::paper_questions(&ctx.pool, &paper).await {
        Ok(v) => v,
        Err(_) => return hx(views::error(&ctx, 500, ""), StatusCode::INTERNAL_SERVER_ERROR).into_response(),
    };
    let title: String = sqlx::query_scalar("SELECT title FROM papers WHERE id=?").bind(&paper).fetch_one(&ctx.pool).await.unwrap_or_else(|_| "Paper".into());
    let duration: i64 = sqlx::query_scalar("SELECT duration_min FROM papers WHERE id=?").bind(&paper).fetch_one(&ctx.pool).await.unwrap_or(30);
    let amap = exam::answers_map(&ctx.pool, &attempt_id).await.unwrap_or_default();
    let answers: Vec<(i64, String, bool)> = questions
        .iter()
        .map(|q| {
            let (sel, rev, _) = amap.get(&q.id).cloned().unwrap_or_default();
            (q.id, sel, rev)
        })
        .collect();
    hx(views::attempt(&ctx, &name, &attempt_id, &questions, &answers, duration, &title), StatusCode::OK).into_response()
}

async fn save_answers(State(ctx): State<CtxArc>, h: HeaderMap, Path(attempt): Path<String>, Json(body): Json<SaveIn>) -> impl IntoResponse {
    let uid = match auth::user_id(&ctx.pool, &h).await {
        Some(u) => u,
        None => return hx("{\"error\":\"sign in first\"}".into(), StatusCode::UNAUTHORIZED),
    };
    match exam::save_answers(&ctx.pool, &attempt, &uid, &body).await {
        Ok(_) => hx("{\"ok\":true}".into(), StatusCode::OK),
        Err(e) => {
            let m = e.to_string();
            hx(format!("{{\"error\":\"{m}\"}}"), StatusCode::NOT_FOUND)
        }
    }
}

async fn attempt_state(State(ctx): State<CtxArc>, h: HeaderMap, Path(attempt): Path<String>) -> impl IntoResponse {
    let uid = match auth::user_id(&ctx.pool, &h).await {
        Some(u) => u,
        None => return hx("{\"error\":\"sign in first\"}".into(), StatusCode::UNAUTHORIZED),
    };
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM attempts WHERE id=? AND user_id=?")
        .bind(&attempt)
        .bind(&uid)
        .fetch_one(&ctx.pool)
        .await
        .unwrap_or(0);
    if n == 0 {
        return hx("{\"error\":\"not found\"}".into(), StatusCode::NOT_FOUND);
    }
    let amap = exam::answers_map(&ctx.pool, &attempt).await.unwrap_or_default();
    let items: Vec<serde_json::Value> = amap
        .iter()
        .map(|(qid, (sel, rev, secs))| serde_json::json!({"question_id": qid, "selected": sel, "marked_review": rev, "time_secs": secs}))
        .collect();
    hx(serde_json::json!({"attempt": attempt, "answers": items}).to_string(), StatusCode::OK)
}

async fn submit(State(ctx): State<CtxArc>, h: HeaderMap, Path(attempt): Path<String>) -> axum::response::Response {
    let (uid, _) = match ctx.require_user(&h).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let owned: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM attempts WHERE id=? AND user_id=?").bind(&attempt).bind(&uid).fetch_one(&ctx.pool).await.unwrap_or(0);
    if owned == 0 {
        return hx(views::error(&ctx, 404, ""), StatusCode::NOT_FOUND).into_response();
    }
    match exam::finalize(&ctx.pool, &attempt).await {
        Ok(_) => Redirect::to(&format!("{}/result/{attempt}", ctx.base)).into_response(),
        Err(e) => {
            let m = e.to_string();
            hx(views::error(&ctx, 500, &m), StatusCode::INTERNAL_SERVER_ERROR).into_response()
        }
    }
}

async fn submit_api(State(ctx): State<CtxArc>, h: HeaderMap, Path(attempt): Path<String>) -> impl IntoResponse {
    let uid = match auth::user_id(&ctx.pool, &h).await {
        Some(u) => u,
        None => return hx("{\"error\":\"sign in first\"}".into(), StatusCode::UNAUTHORIZED),
    };
    let owned: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM attempts WHERE id=? AND user_id=?").bind(&attempt).bind(&uid).fetch_one(&ctx.pool).await.unwrap_or(0);
    if owned == 0 {
        return hx("{\"error\":\"not found\"}".into(), StatusCode::NOT_FOUND);
    }
    match exam::finalize(&ctx.pool, &attempt).await {
        Ok((score, max, used)) => hx(serde_json::json!({"score": score, "max": max, "used_secs": used, "result_url": format!("{}/result/{attempt}", ctx.base)}).to_string(), StatusCode::OK),
        Err(e) => {
            let m = e.to_string();
            hx(format!("{{\"error\":\"{m}\"}}"), StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn result_page(State(ctx): State<CtxArc>, h: HeaderMap, Path(attempt): Path<String>) -> axum::response::Response {
    let (uid, name) = match ctx.require_user(&h).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let owned: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM attempts WHERE id=? AND user_id=? AND submitted_at IS NOT NULL").bind(&attempt).bind(&uid).fetch_one(&ctx.pool).await.unwrap_or(0);
    if owned == 0 {
        return hx(views::error(&ctx, 404, ""), StatusCode::NOT_FOUND).into_response();
    }
    let (rows, score, max, used) = match exam::grade(&ctx.pool, &attempt).await {
        Ok(v) => v,
        Err(e) => {
            let m = e.to_string();
            return hx(views::error(&ctx, 500, &m), StatusCode::INTERNAL_SERVER_ERROR).into_response();
        }
    };
    let a = analyse(&rows, score, max, used);
    let rv: Vec<views::RowView> = rows
        .iter()
        .map(|r| views::RowView { q: &r.q, selected: &r.selected, marks: r.marks, correct: r.correct, time_secs: r.time_secs })
        .collect();
    hx(views::result(&ctx, &name, &attempt, &a, &rv), StatusCode::OK).into_response()
}

async fn history_page(State(ctx): State<CtxArc>, h: HeaderMap) -> axum::response::Response {
    let (uid, name) = match ctx.require_user(&h).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let tests = analytics::history(&ctx.pool, &uid).await.unwrap_or_default();
    hx(views::history(&ctx, &name, &tests), StatusCode::OK).into_response()
}

async fn dashboard_page(State(ctx): State<CtxArc>, h: HeaderMap) -> axum::response::Response {
    let (uid, name) = match ctx.require_user(&h).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let tests = analytics::history(&ctx.pool, &uid).await.unwrap_or_default();
    let weak = analytics::weakest(&ctx.pool, &uid).await.unwrap_or_default();
    let marks: i32 = tests.iter().map(|t| t.score).sum();
    let gained: i32 = tests.iter().map(|t| t.right).sum();
    let lost: i32 = tests.iter().map(|t| t.wrong).sum();
    let notes = coach_notes(
        &analytics::Analysis {
            score: tests.first().map(|t| t.score).unwrap_or(0),
            max_score: tests.first().map(|t| t.max_score).unwrap_or(0),
            used_secs: tests.first().map(|t| t.used_secs).unwrap_or(0),
            right: tests.first().map(|t| t.right).unwrap_or(0),
            wrong: tests.first().map(|t| t.wrong).unwrap_or(0),
            skipped: 0,
            by_subject: vec![],
            by_difficulty: vec![],
            by_topic: vec![],
            mistakes: vec![],
        },
        &tests,
    );
    hx(views::dashboard(&ctx, &name, &tests, &weak, &notes, (tests.len() as i32, marks, gained, lost)), StatusCode::OK).into_response()
}

async fn shared_paper(State(ctx): State<CtxArc>, Path(token): Path<String>) -> axum::response::Response {
    let row = sqlx::query("SELECT id, title, duration_min FROM papers WHERE share_token=?").bind(&token).fetch_optional(&ctx.pool).await;
    let (id, title, duration) = match row {
        Ok(Some(r)) => {
            use sqlx::Row;
            (r.get::<String, _>("id"), r.get::<String, _>("title"), r.get::<i64, _>("duration_min"))
        }
        _ => return hx(views::error(&ctx, 404, ""), StatusCode::NOT_FOUND).into_response(),
    };
    let questions = match exam::paper_questions(&ctx.pool, &id).await {
        Ok(v) => v,
        Err(_) => return hx(views::error(&ctx, 500, ""), StatusCode::INTERNAL_SERVER_ERROR).into_response(),
    };
    hx(views::shared(&ctx, &title, duration, &questions), StatusCode::OK).into_response()
}

const TERMS: &str = include_str!("../content/terms.html");
const PRIVACY: &str = include_str!("../content/privacy.html");