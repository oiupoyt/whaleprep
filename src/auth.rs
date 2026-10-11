use axum::extract::{Form, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse};
use chrono::Utc;
use serde::Deserialize;
use sqlx::SqlitePool;
use std::sync::Arc;

use crate::views;

pub const COOKIE: &str = "pyq_session";

pub struct Ctx {
    pub pool: SqlitePool,
    pub base: String,
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
    match sqlx::query_scalar::<_, String>("SELECT user_id FROM sessions WHERE token=?")
        .bind(token)
        .fetch_optional(pool)
        .await
    {
        Ok(found) => {
            tracing::debug!("session lookup -> {:?}", found);
            found
        }
        Err(e) => {
            tracing::error!("session lookup failed: {e}");
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
        return html_page(views::student(&ctx, &next, Some("Enter the name your account shows.")), StatusCode::BAD_REQUEST);
    }
    if !email.contains('@') || email.len() > 120 {
        return html_page(views::student(&ctx, &next, Some("That email address does not look right.")), StatusCode::BAD_REQUEST);
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