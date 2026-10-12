use axum::extract::{Form, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse};
use chrono::Utc;
use serde::Deserialize;
use sqlx::SqlitePool;
use std::sync::Arc;

use crate::google::Google;
use crate::views;

pub const COOKIE: &str = "pyq_session";

pub struct Ctx {
    pub pool: SqlitePool,
    pub base: String,
    pub google_client_id: String,
    pub google: Option<Arc<Google>>,
    /// Landing-page bank counts, refreshed at most once a minute. They only
    /// change on import, so there is no reason to run five aggregate
    /// subselects on every homepage hit.
    pub stats_cache: std::sync::Mutex<(std::time::Instant, (i64, i64, i64, i64, i64))>,
    /// Autosave rate limiter: user id -> (window start, hits in window).
    /// The exam screen autosaves on every answer, so a stuck client retrying
    /// in a tight loop could otherwise hammer SQLite's single writer.
    pub save_hits: std::sync::Mutex<std::collections::HashMap<String, (std::time::Instant, u32)>>,
}

impl Ctx {
    /// True when this user may autosave now (30 saves per rolling minute).
    pub fn save_allowed(&self, uid: &str) -> bool {
        const LIMIT: u32 = 30;
        const WINDOW: std::time::Duration = std::time::Duration::from_secs(60);
        let mut guard = self.save_hits.lock().unwrap();
        // Opportunistic cleanup so the map cannot grow without bound.
        if guard.len() > 4096 {
            let now = std::time::Instant::now();
            guard.retain(|_, (t, _)| now.duration_since(*t) < WINDOW * 2);
        }
        let entry = guard.entry(uid.to_string()).or_insert((std::time::Instant::now(), 0));
        if entry.0.elapsed() >= WINDOW {
            *entry = (std::time::Instant::now(), 1);
            return true;
        }
        if entry.1 >= LIMIT {
            return false;
        }
        entry.1 += 1;
        true
    }

    pub async fn bank_stats(&self) -> (i64, i64, i64, i64, i64) {
        {
            let guard = self.stats_cache.lock().unwrap();
            if guard.0.elapsed() < std::time::Duration::from_secs(60) {
                return guard.1;
            }
        }
        let fresh: (i64, i64, i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM questions),(SELECT COUNT(DISTINCT subject||'-'||year||'-'||shift) FROM questions),(SELECT COUNT(*) FROM subtopics),(SELECT COUNT(DISTINCT subject) FROM questions),(SELECT COUNT(*) FROM users)",
        )
        .fetch_one(&self.pool)
        .await
        .unwrap_or((0, 0, 0, 0, 0));
        *self.stats_cache.lock().unwrap() =
            (std::time::Instant::now(), fresh);
        fresh
    }
}

impl Ctx {
    /// Returns the signed-in user id and display name, or a redirect to sign-in.
    pub async fn require_user(&self, h: &HeaderMap) -> Result<(String, String), axum::response::Response> {
        match user_id(&self.pool, h).await {
            Some(id) => {
                let name: String = sqlx::query_scalar("SELECT name FROM users WHERE id=?")
                    .bind(&id)
                    .fetch_one(&self.pool)
                    .await
                    .unwrap_or_else(|_| "Student".into());
                Ok((id, name))
            }
            None => Err(redirect(&format!("{}/student", self.base))),
        }
    }
}

pub async fn user_id(pool: &SqlitePool, h: &HeaderMap) -> Option<String> {
    let raw = h.get(COOKIE).and_then(|v| v.to_str().ok()).or_else(|| {
        h.get(axum::http::header::COOKIE)
            .and_then(|v| v.to_str().ok())
    })?;
    let mut token: Option<&str> = None;
    for part in raw.split(';') {
        if let Some((k, v)) = part.trim().split_once('=')
            && k == COOKIE
        {
            token = Some(v);
        }
    }
    let token = token?;
    tracing::debug!("session token={token}");
    // Sessions live 30 days (matching the cookie Max-Age). Stale rows are
    // deleted on sight so repeat sign-ins don't orphan tokens forever.
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT user_id, created_at FROM sessions WHERE token=?")
            .bind(token)
            .fetch_optional(pool)
            .await
            .unwrap_or(None);
    match row {
        Some((uid, created)) => {
            let fresh = chrono::DateTime::parse_from_rfc3339(&created)
                .map(|t| (chrono::Utc::now() - t.with_timezone(&chrono::Utc)).num_days() < 30)
                .unwrap_or(false);
            if !fresh {
                sqlx::query("DELETE FROM sessions WHERE token=?")
                    .bind(token)
                    .execute(pool)
                    .await
                    .ok();
                return None;
            }
            tracing::debug!("session lookup -> {:?}", uid);
            Some(uid)
        }
        None => {
            tracing::debug!("session lookup -> None");
            None
        }
    }
}

pub fn redirect(to: &str) -> axum::response::Response {
    axum::response::Response::builder()
        .status(StatusCode::FOUND)
        .header(axum::http::header::LOCATION, to)
        .body(axum::body::Body::empty())
        .expect("static redirect response")
}

pub fn html_page(body: String, code: StatusCode) -> axum::response::Response {
    (code, Html(body)).into_response()
}

pub async fn student_post(State(ctx): State<Arc<Ctx>>, Form(f): Form<StudentForm>) -> axum::response::Response {
    let name = f.name.trim();
    let email = f.email.trim().to_lowercase();
    let next = f
        .next
        .filter(|n| n.starts_with('/') && !n.contains(".."))
        .unwrap_or_else(|| "/build".into());

    if name.is_empty() || name.chars().count() > 80 {
        return html_page(views::student(&ctx, &next, Some("Enter the name your account shows."), None), StatusCode::BAD_REQUEST);
    }
    if !email.contains('@') || email.len() > 120 {
        return html_page(views::student(&ctx, &next, Some("That email address does not look right."), None), StatusCode::BAD_REQUEST);
    }

    let candidate = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO users(id,name,email,created_at) VALUES(?,?,?,?) ON CONFLICT(email) DO UPDATE SET name=excluded.name")
        .bind(&candidate)
        .bind(name)
        .bind(&email)
        .bind(Utc::now().to_rfc3339())
        .execute(&ctx.pool)
        .await
        .ok();
    let uid: String = sqlx::query_scalar("SELECT id FROM users WHERE email=?")
        .bind(&email)
        .fetch_one(&ctx.pool)
        .await
        .unwrap_or(candidate);

    let token = uuid::Uuid::new_v4().simple().to_string();
    sqlx::query("INSERT INTO sessions(token,user_id,created_at) VALUES(?,?,?)")
        .bind(&token)
        .bind(&uid)
        .bind(Utc::now().to_rfc3339())
        .execute(&ctx.pool)
        .await
        .ok();

    let cookie = format!("{COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age=2592000");
    axum::response::Response::builder()
        .status(StatusCode::FOUND)
        .header(axum::http::header::LOCATION, format!("{}{}", ctx.base, next))
        .header(axum::http::header::SET_COOKIE, cookie)
        .body(axum::body::Body::empty())
        .expect("static redirect response")
}

/// Creates a session for a verified identity and returns the redirect that carries it.
pub async fn start_session(ctx: &Ctx, uid: &str, name: &str, email: &str, next: Option<String>) -> axum::response::Response {
    let _ = sqlx::query("UPDATE users SET name=?, email=? WHERE id=?").bind(name).bind(email).bind(uid).execute(&ctx.pool).await;
    let token = uuid::Uuid::new_v4().simple().to_string();
    if let Err(e) = sqlx::query("INSERT INTO sessions(token,user_id,created_at) VALUES(?,?,?)")
        .bind(&token)
        .bind(uid)
        .bind(Utc::now().to_rfc3339())
        .execute(&ctx.pool)
        .await
    {
        tracing::error!("could not start session: {e}");
    }
    tracing::info!("sign in: {email}");
    let to = next.filter(|n| n.starts_with('/') && !n.contains("..")).unwrap_or_else(|| "/build".into());
    axum::response::Response::builder()
        .status(StatusCode::FOUND)
        .header(axum::http::header::LOCATION, format!("{}{}", ctx.base, to))
        .header(
            axum::http::header::SET_COOKIE,
            format!("{COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age=2592000"),
        )
        .body(axum::body::Body::empty())
        .expect("static redirect response")
}

#[derive(Deserialize)]
pub struct StudentForm {
    pub name: String,
    pub email: String,
    #[serde(default)]
    pub next: Option<String>,
}

pub async fn logout(State(ctx): State<Arc<Ctx>>, h: HeaderMap) -> axum::response::Response {
    if let Some(uid) = user_id(&ctx.pool, &h).await {
        tracing::info!("sign out for {uid}");
    }
    if let Some(raw) = h.get(COOKIE).and_then(|c| c.to_str().ok())
        && let Some((_, v)) = raw.split_once('=')
    {
        let token = v.split(';').next().unwrap_or_default();
        sqlx::query("DELETE FROM sessions WHERE token=?").bind(token).execute(&ctx.pool).await.ok();
    }
    let mut r = redirect(&format!("{}/", ctx.base));
    r.headers_mut().insert(
        axum::http::header::SET_COOKIE,
        format!("{COOKIE}=; Path=/; HttpOnly; Max-Age=0").parse().expect("valid cookie"),
    );
    r
}