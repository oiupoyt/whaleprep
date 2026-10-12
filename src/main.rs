mod analytics;
mod auth;
mod db;
mod exam;
mod google;
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
    // Defaults to info; RUST_LOG=debug adds the query log.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "whaleprep=info,sqlx=warn".into()),
        )
        .init();
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
    let client_id = std::env::var("GOOGLE_CLIENT_ID").unwrap_or_default();
    let client_secret = std::env::var("GOOGLE_CLIENT_SECRET").unwrap_or_default();
    let google = if client_id.is_empty() {
        tracing::warn!("GOOGLE_CLIENT_ID is unset: the Google button stays hidden and only local sign-in works");
        None
    } else {
        if client_secret.is_empty() {
            tracing::warn!("GOOGLE_CLIENT_SECRET is unset: the popup flow still works, the full-page fallback does not");
        }
        tracing::info!("Google sign-in enabled for {}", &client_id[..client_id.len().min(16)]);
        Some(Arc::new(google::Google::new(&client_id, &client_secret)))
    };
    let ctx = Arc::new(Ctx { pool, base: base.clone(), google_client_id: client_id, google });
    let static_dir = format!("{}/static", std::env::current_dir()?.display());

    let app = Router::new()
        .route("/", get(home))
        .route("/student", get(student_get).post(student_post))
        .route("/student/google", post(google_callback))
        .route("/student/google/callback", get(google_redirect))
        .route("/logout", get(auth::logout))
        .route("/terms", get(terms))
        .route("/privacy", get(privacy))
        .route("/build", get(build_page).post(build_post))
        .route("/attempt/{paper}", get(attempt_page).post(start_attempt))
        .route("/attempt/{attempt}/answer", post(save_answers))
        .route("/attempt/{attempt}/submit", post(submit))
        .route("/result/{attempt}", get(result_page))
        .route("/result/{attempt}/tag/{qid}", post(tag_toggle))
        .route("/history", get(history_page))
        .route("/dashboard", get(dashboard_page))
        .route("/s/{token}", get(shared_paper))
        .route("/paper/{id}", get(paper_page))
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
    // The fallback needs a PKCE verifier bound to this visit, held here rather than
    // in a cookie so it cannot be read back by script.
    let mut fallback = None;
    if ctx.google.is_some() {
        let state = google::new_state();
        let verifier = google::new_verifier();
        sqlx::query("INSERT INTO pkce(state, verifier, created_at) VALUES(?,?,?)")
            .bind(&state)
            .bind(&verifier)
            .bind(chrono::Utc::now().to_rfc3339())
            .execute(&ctx.pool)
            .await
            .ok();
        let uri = format!("{}{}/student/google/callback", public_origin(&h), ctx.base);
        fallback = Some((state, google::challenge(&verifier), uri));
    }
    hx(views::student(&ctx, &next, None, fallback), StatusCode::OK).into_response()
}

/// Scheme and host for the callback Google redirects back to, derived from the
/// request so it works on a port or behind a tunnel without extra configuration.
fn public_origin(h: &HeaderMap) -> String {
    let host = h
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost");
    let scheme = if h
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.split(',').next().unwrap_or("http").trim() == "https")
    {
        "https"
    } else {
        "http"
    };
    format!("{scheme}://{host}")
}

async fn student_post(State(ctx): State<CtxArc>, Form(f): Form<auth::StudentForm>) -> axum::response::Response {
    auth::student_post(State(ctx), axum::Form(f)).await
}

#[derive(serde::Deserialize)]
struct GoogleForm {
    credential: String,
    #[serde(default)]
    next: Option<String>,
}

#[derive(serde::Deserialize)]
struct GoogleRedirect {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

/// Verifies the ID token Google issued, then attaches the account to a session.
/// A failure here is never trusted client-side: the token is re-checked here.
async fn google_callback(State(ctx): State<CtxArc>, Form(f): Form<GoogleForm>) -> axum::response::Response {
    let Some(g) = ctx.google.clone() else {
        return auth::html_page(
            views::student(&ctx, "/build", Some("Google sign-in is not configured on this server."), None),
            StatusCode::BAD_REQUEST,
        )
        .into_response();
    };

    let claims = match g.verify(&f.credential).await {
        Ok(c) => c,
        Err(msg) => {
            tracing::warn!("google sign-in rejected: {msg}");
            return auth::html_page(views::student(&ctx, f.next.as_deref().unwrap_or("/build"), Some(&msg), None), StatusCode::UNAUTHORIZED)
                .into_response();
        }
    };
    sign_in_with_google(&ctx, &claims, f.next).await
}

/// The redirect fallback: Google sends the authorization code back here, and it is
/// redeemed server side so no secret ever reaches the browser.
async fn google_redirect(State(ctx): State<CtxArc>, h: HeaderMap, Query(q): Query<GoogleRedirect>) -> axum::response::Response {
    let fail = |m: &str| auth::html_page(views::student(&ctx, "/build", Some(m), None), StatusCode::BAD_REQUEST).into_response();
    let Some(g) = ctx.google.clone() else {
        return fail("Google sign-in is not configured on this server.");
    };
    if let Some(e) = q.error.as_deref() {
        return fail(&format!("Google declined that sign-in: {e}"));
    }
    let (Some(code), Some(state)) = (q.code, q.state) else {
        return fail("That sign-in link is incomplete. Try again.");
    };
    // The state must match one issued here, and it is spent on first use.
    let verifier: Option<String> = sqlx::query_scalar(
        "SELECT verifier FROM pkce WHERE state=? AND created_at > ?",
    )
    .bind(&state)
    .bind((chrono::Utc::now() - chrono::Duration::minutes(10)).to_rfc3339())
    .fetch_optional(&ctx.pool)
    .await
    .unwrap_or(None);
    sqlx::query("DELETE FROM pkce WHERE state=?").bind(&state).execute(&ctx.pool).await.ok();
    let Some(verifier) = verifier else {
        return fail("That sign-in link has expired or was not started here.");
    };

    let redirect_uri = format!("{}{}/student/google/callback", public_origin(&h), ctx.base);
    let claims = match g.exchange_code(&code, &verifier, &redirect_uri).await {
        Ok(c) => c,
        Err(msg) => {
            tracing::warn!("google code exchange failed: {msg}");
            return auth::html_page(views::student(&ctx, "/build", Some(&msg), None), StatusCode::UNAUTHORIZED).into_response();
        }
    };
    sign_in_with_google(&ctx, &claims, None).await
}

async fn sign_in_with_google(ctx: &CtxArc, claims: &google::Claims, next: Option<String>) -> axum::response::Response {
    let email = claims.email.to_lowercase();
    let name = claims.display_name();
    let candidate = uuid::Uuid::new_v4().to_string();
    if let Err(e) = sqlx::query("INSERT INTO users(id,name,email,created_at) VALUES(?,?,?,?) ON CONFLICT(email) DO UPDATE SET name=excluded.name")
        .bind(&candidate)
        .bind(&name)
        .bind(&email)
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&ctx.pool)
        .await
    {
        tracing::error!("could not store google user: {e}");
    }
    let uid: String = sqlx::query_scalar("SELECT id FROM users WHERE email=?")
        .bind(&email)
        .fetch_one(&ctx.pool)
        .await
        .unwrap_or(candidate);

    auth::start_session(ctx, &uid, &name, &email, next).await
}

async fn load_chapter_blocks(pool: &sqlx::SqlitePool) -> Vec<views::SubjectBlock> {
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT c.id, c.subject, c.name, s.id AS sub_id, s.name AS sub, COALESCE(qc.n,0) AS n FROM chapters c JOIN subtopics s ON s.chapter_id=c.id LEFT JOIN (SELECT chapter_id, COUNT(*) AS n FROM questions GROUP BY chapter_id) qc ON qc.chapter_id=c.id ORDER BY c.subject, c.name, s.name",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    let mut blocks: Vec<views::SubjectBlock> = Vec::new();
    for r in &rows {
        let subject: String = r.get("subject");
        let id: i64 = r.get("id");
        let cname: String = r.get("name");
        let sub_id: i64 = r.get("sub_id");
        let sub: String = r.get("sub");
        let n: i64 = r.get("n");
        if !blocks.iter().any(|b| b.subject == subject) {
            blocks.push(views::SubjectBlock { subject: subject.clone(), chapters: Vec::new() });
        }
        let blk = blocks.iter_mut().find(|b| b.subject == subject).unwrap();
        match blk.chapters.iter_mut().find(|c| c.id == id) {
            Some(c) => { c.subtopic_ids.push(sub_id); c.subtopics.push(sub); c.count = n; }
            None => blk.chapters.push(views::ChapterBlock {
                id, name: cname, subtopic_ids: vec![sub_id], subtopics: vec![sub], count: n,
            }),
        }
    }
    blocks
}


async fn build_page(State(ctx): State<CtxArc>, h: HeaderMap) -> axum::response::Response {
    let (_uid, name) = match ctx.require_user(&h).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let blocks = load_chapter_blocks(&ctx.pool).await;
    let available: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM questions").fetch_one(&ctx.pool).await.unwrap_or(0);
    let form = BuildForm { chapters: String::new(), subtopics: String::new(), count: Some(12), duration: Some(24), unseen_only: Some("on".into()), mode: String::new() };
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
            let blocks = load_chapter_blocks(&ctx.pool).await;
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
    // Existing tags per question so the form shows current state.
    let tag_rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT question_id, tag FROM mistake_tags WHERE attempt_id=?",
    )
    .bind(&attempt)
    .fetch_all(&ctx.pool)
    .await
    .unwrap_or_default();
    let rv: Vec<views::RowView> = rows
        .iter()
        .map(|r| {
            let tags: Vec<String> = tag_rows
                .iter()
                .filter(|(qid, _)| *qid == r.q.id)
                .map(|(_, t)| t.clone())
                .collect();
            views::RowView {
                q: &r.q,
                selected: &r.selected,
                marks: r.marks,
                correct: r.correct,
                time_secs: r.time_secs,
                attempt_id: &attempt,
                tags,
            }
        })
        .collect();
    hx(views::result(&ctx, &name, &attempt, &a, &rv), StatusCode::OK).into_response()
}

#[derive(serde::Deserialize)]
struct TagForm {
    tag: String,
}

async fn tag_toggle(
    State(ctx): State<CtxArc>,
    h: HeaderMap,
    Path((attempt, qid)): Path<(String, i64)>,
    Form(f): Form<TagForm>,
) -> axum::response::Response {
    let (uid, _name) = match ctx.require_user(&h).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let owned: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM attempts WHERE id=? AND user_id=?")
        .bind(&attempt)
        .bind(&uid)
        .fetch_one(&ctx.pool)
        .await
        .unwrap_or(0);
    if owned == 0 {
        return hx(views::error(&ctx, 404, ""), StatusCode::NOT_FOUND).into_response();
    }
    if !["concept", "calculation", "misread", "time", "guess"].contains(&f.tag.as_str()) {
        return Redirect::to(&format!("{}/result/{attempt}", ctx.base)).into_response();
    }
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mistake_tags WHERE attempt_id=? AND question_id=? AND tag=?")
        .bind(&attempt)
        .bind(qid)
        .bind(&f.tag)
        .fetch_one(&ctx.pool)
        .await
        .unwrap_or(0);
    if n > 0 {
        sqlx::query("DELETE FROM mistake_tags WHERE attempt_id=? AND question_id=? AND tag=?")
            .bind(&attempt).bind(qid).bind(&f.tag)
            .execute(&ctx.pool).await.ok();
    } else {
        sqlx::query("INSERT OR IGNORE INTO mistake_tags(attempt_id,question_id,tag) VALUES(?,?,?)")
            .bind(&attempt).bind(qid).bind(&f.tag)
            .execute(&ctx.pool).await.ok();
    }
    Redirect::to(&format!("{}/result/{attempt}", ctx.base)).into_response()
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
    let mistakes = analytics::mistake_rollup(&ctx.pool, &uid).await;
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
    hx(views::dashboard(&ctx, &name, &tests, &weak, &notes, (tests.len() as i32, marks, gained, lost), &mistakes), StatusCode::OK).into_response()
}

/// Owner view of a paper: redirects to the attempt screen so the paper can
/// be taken. Private — only the owning user may resolve their paper id.
async fn paper_page(State(ctx): State<CtxArc>, h: HeaderMap, Path(id): Path<String>) -> axum::response::Response {
    let (uid, _name) = match ctx.require_user(&h).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let owned: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM papers WHERE id=? AND user_id=?")
        .bind(&id)
        .bind(&uid)
        .fetch_one(&ctx.pool)
        .await
        .unwrap_or(0);
    if owned == 0 {
        return hx(views::error(&ctx, 404, ""), StatusCode::NOT_FOUND).into_response();
    }
    Redirect::to(&format!("{}/attempt/{id}", ctx.base)).into_response()
}

async fn shared_paper(State(ctx): State<CtxArc>, h: HeaderMap, Path(token): Path<String>) -> axum::response::Response {
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
    let signed = auth::user_id(&ctx.pool, &h).await.is_some();
    hx(views::shared(&ctx, &title, duration, &questions, &id, &token, signed), StatusCode::OK).into_response()
}

const TERMS: &str = include_str!("../content/terms.html");
const PRIVACY: &str = include_str!("../content/privacy.html");