use std::collections::HashMap;

use chrono::Utc;
use serde::Deserialize;
use sqlx::{Row, SqlitePool};

pub const CORRECT: i32 = 4;
pub const NEGATIVE: i32 = -1;

#[derive(Clone)]
pub struct Question {
    pub id: i64,
    pub subject: String,
    pub chapter: String,
    pub subtopic: String,
    pub qtype: String,
    pub stem: String,
    pub opts: [String; 4],
    pub answer: String,
    pub difficulty: String,
    pub year: i64,
    pub shift: String,
    pub solution: String,
}

pub fn qrow(r: &sqlx::sqlite::SqliteRow) -> Question {
    let g = |i: &str| -> String { r.try_get::<Option<String>, _>(i).ok().flatten().unwrap_or_default() };
    Question {
        id: r.get("id"),
        subject: r.get("subject"),
        chapter: g("chapter"),
        subtopic: g("subtopic"),
        qtype: r.get("qtype"),
        stem: r.get("stem"),
        opts: [g("opt_a"), g("opt_b"), g("opt_c"), g("opt_d")],
        answer: r.get("answer"),
        difficulty: r.get("difficulty"),
        year: r.get("year"),
        shift: r.get("shift"),
        solution: g("solution"),
    }
}

/// Both `chapters` and `subtopics` arrive comma-separated (repeated keys lose all
/// but the last value in urlencoded bodies, so they are joined into one field each).
#[derive(Deserialize)]
pub struct BuildForm {
    #[serde(default)]
    pub chapters: String,
    /// When non-empty, only questions from these subtopic ids are drawn.
    #[serde(default)]
    pub subtopics: String,
    pub count: Option<usize>,
    pub duration: Option<i64>,
    pub unseen_only: Option<String>,
    /// "jee" means 3 subjects × 60 min each; otherwise one flat timer.
    #[serde(default)]
    pub mode: String,
}

impl BuildForm {
    pub fn chapter_ids(&self) -> Vec<i64> {
        self.chapters.split(',').filter_map(|c| c.trim().parse().ok()).collect()
    }
    pub fn subtopic_ids(&self) -> Vec<i64> {
        self.subtopics.split(',').filter_map(|c| c.trim().parse().ok()).collect()
    }
    pub fn is_jee_mode(&self) -> bool {
        self.mode == "jee"
    }
}

pub async fn build_paper(pool: &SqlitePool, uid: &str, f: &BuildForm) -> anyhow::Result<(String, String)> {
    let chapters = f.chapter_ids();
    if chapters.is_empty() {
        anyhow::bail!("Pick at least one chapter");
    }
    let subtopics = f.subtopic_ids();
    let count = f.count.unwrap_or(12).clamp(3, 75);
    let unseen = f.unseen_only.as_deref() == Some("on");

    // JEE Main: each subject gets 60 minutes.
    // JEE mode: 60 min per unique subject across the selected chapters.
    // Custom: caller picks a flat duration; default is 2 min/question.
    let duration = if f.is_jee_mode() {
        let mut subj_set = std::collections::HashSet::<String>::new();
        for ch in &chapters {
            if let Ok(Some(s)) = sqlx::query_scalar::<_, String>("SELECT subject FROM chapters WHERE id=?")
                .bind(ch).fetch_optional(pool).await { subj_set.insert(s); }
        }
        60 * subj_set.len().max(1) as i64
    } else {
        f.duration.unwrap_or(count as i64 * 2).clamp(5, 180)
    };

    let mut ids: Vec<i64> = Vec::new();
    // Chapters × subtopics: one indexed lookup per pair (sqlx needs a static
    // string per branch, hence the unseen/not split inline).
    for ch in &chapters {
        if !subtopics.is_empty() {
            for st in &subtopics {
                let rows: Vec<sqlx::sqlite::SqliteRow> = if unseen {
                    sqlx::query(
                        "SELECT q.id FROM questions q                          WHERE q.chapter_id=? AND q.subtopic_id=?                          AND q.id NOT IN (SELECT question_id FROM seen_questions WHERE user_id=?)"
                    ).bind(ch).bind(st).bind(uid).fetch_all(pool).await?
                } else {
                    sqlx::query(
                        "SELECT q.id FROM questions q WHERE q.chapter_id=? AND q.subtopic_id=?"
                    ).bind(ch).bind(st).fetch_all(pool).await?
                };
                for r in rows { ids.push(r.get::<i64, _>("id")); }
            }
        } else {
            let rows: Vec<sqlx::sqlite::SqliteRow> = if unseen {
                sqlx::query(
                    "SELECT q.id FROM questions q WHERE q.chapter_id=?                      AND q.id NOT IN (SELECT question_id FROM seen_questions WHERE user_id=?)"
                ).bind(ch).bind(uid).fetch_all(pool).await?
            } else {
                sqlx::query("SELECT q.id FROM questions q WHERE q.chapter_id=?")
                    .bind(ch).fetch_all(pool).await?
            };
            for r in rows { ids.push(r.get::<i64, _>("id")); }
        }
    }
    // Deduplicate: a question can match multiple subtopics if the form sent both.
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        anyhow::bail!("No questions left for that selection. Turn off 'unseen only' to reuse questions.");
    }
    let seed = uid.len() as u64 ^ chapters.iter().sum::<i64>() as u64 ^ count as u64;
    shuffle(&mut ids, seed);
    ids.truncate(count);

    let paper = uuid::Uuid::new_v4().to_string();
    let share = uuid::Uuid::new_v4().simple().to_string()[..10].to_string();
    // Derive a title: fetch the subject for each chapter that contributed questions.
    let chapter_subjects: std::collections::HashSet<String> = {
        let mut hs = std::collections::HashSet::new();
        for ch in &chapters {
            let subj: Option<String> = sqlx::query_scalar("SELECT subject FROM chapters WHERE id=?")
                .bind(ch).fetch_optional(pool).await.unwrap_or_default();
            if let Some(s) = subj { hs.insert(s); }
        }
        hs
    };
    let title = if chapter_subjects.len() == 1 {
        chapter_subjects.iter().next().map(|s| format!("{s} paper · {count} questions")).unwrap_or_else(|| format!("Mixed paper · {count} questions"))
    } else {
        format!("Mixed paper · {count} questions")
    };

    sqlx::query("INSERT INTO papers(id,user_id,title,duration_min,created_at,share_token) VALUES(?,?,?,?,?,?)")
        .bind(&paper).bind(uid).bind(&title).bind(duration)
        .bind(Utc::now().to_rfc3339()).bind(&share)
        .execute(pool).await?;
    for (i, qid) in ids.iter().enumerate() {
        sqlx::query("INSERT INTO paper_questions(paper_id,question_id,ord) VALUES(?,?,?)")
            .bind(&paper).bind(qid).bind(i as i64)
            .execute(pool).await?;
    }
    Ok((paper, share))
}

fn shuffle(v: &mut Vec<i64>, mut seed: u64) {
    if seed == 0 {
        seed = 0x9e3779b97f4a7c15;
    }
    for i in (1..v.len()).rev() {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let j = (seed % (i as u64 + 1)) as usize;
        v.swap(i, j);
    }
}

pub fn numerical_ok(selected: &str, answer: &str) -> bool {
    let s: f64 = match selected.trim().parse() {
        Ok(v) => v,
        Err(_) => return false,
    };
    let a: f64 = match answer.trim().parse() {
        Ok(v) => v,
        Err(_) => return false,
    };
    let tol = a.abs() * 1e-3 + 1e-6;
    (s - a).abs() <= tol
}

pub fn is_correct(q: &Question, selected: &str) -> bool {
    if selected.trim().is_empty() {
        return false;
    }
    if q.qtype == "NUM" {
        numerical_ok(selected, &q.answer)
    } else {
        selected.trim().eq_ignore_ascii_case(q.answer.trim())
    }
}

pub async fn start_attempt(pool: &SqlitePool, uid: &str, paper: &str) -> anyhow::Result<String> {
    let existing: Option<String> =
        sqlx::query_scalar("SELECT a.id FROM attempts a WHERE a.paper_id=? AND a.user_id=? AND a.submitted_at IS NULL")
            .bind(paper)
            .bind(uid)
            .fetch_optional(pool)
            .await?;
    if let Some(id) = existing {
        return Ok(id);
    }
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO attempts(id,paper_id,user_id,started_at) VALUES(?,?,?,?)")
        .bind(&id)
        .bind(paper)
        .bind(uid)
        .bind(Utc::now().to_rfc3339())
        .execute(pool)
        .await?;
    let qids: Vec<i64> = sqlx::query_scalar("SELECT question_id FROM paper_questions WHERE paper_id=? ORDER BY ord")
        .bind(paper)
        .fetch_all(pool)
        .await?;
    for qid in &qids {
        sqlx::query("INSERT INTO answers(attempt_id,question_id) VALUES(?,?)")
            .bind(&id)
            .bind(qid)
            .execute(pool)
            .await?;
    }
    Ok(id)
}

#[derive(Deserialize)]
pub struct AnswerIn {
    pub question_id: i64,
    pub selected: String,
    pub marked_review: Option<bool>,
    pub time_secs: Option<i64>,
}

#[derive(Deserialize)]
pub struct SaveIn {
    pub answers: Vec<AnswerIn>,
}

pub async fn save_answers(pool: &SqlitePool, attempt: &str, uid: &str, body: &SaveIn) -> anyhow::Result<()> {
    let ok: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM attempts WHERE id=? AND user_id=?")
        .bind(attempt)
        .bind(uid)
        .fetch_one(pool)
        .await?;
    if ok == 0 {
        anyhow::bail!("Attempt not found");
    }
    for a in &body.answers {
        sqlx::query("UPDATE answers SET selected=?, marked_review=?, time_secs=? WHERE attempt_id=? AND question_id=?")
            .bind(&a.selected)
            .bind(a.marked_review.unwrap_or(false) as i64)
            .bind(a.time_secs.unwrap_or(0).clamp(0, 7200))
            .bind(attempt)
            .bind(a.question_id)
            .execute(pool)
            .await?;
    }
    Ok(())
}

pub async fn paper_questions(pool: &SqlitePool, paper: &str) -> anyhow::Result<Vec<Question>> {
    let rows = sqlx::query(
        "SELECT q.*, c.name AS chapter, s.name AS subtopic FROM questions q
         JOIN chapters c ON c.id=q.chapter_id JOIN subtopics s ON s.id=q.subtopic_id
         JOIN paper_questions pq ON pq.question_id=q.id WHERE pq.paper_id=? ORDER BY pq.ord",
    )
    .bind(paper)
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(qrow).collect())
}

pub async fn answers_map(pool: &SqlitePool, attempt: &str) -> anyhow::Result<HashMap<i64, (String, bool, i64)>> {
    let rows = sqlx::query("SELECT question_id, selected, marked_review, time_secs FROM answers WHERE attempt_id=?")
        .bind(attempt)
        .fetch_all(pool)
        .await?;
    Ok(rows
        .iter()
        .map(|r| (r.get::<i64, _>("question_id"), (r.get::<String, _>("selected"), r.get::<i64, _>("marked_review") != 0, r.get::<i64, _>("time_secs"))))
        .collect())
}

pub async fn history_counts(pool: &SqlitePool, attempt: &str) -> anyhow::Result<(usize, i32, i32)> {
    let (rows, _, _, _) = grade(pool, attempt).await?;
    let right = rows.iter().filter(|r| !r.selected.trim().is_empty() && r.correct).count();
    let wrong = rows.iter().filter(|r| !r.selected.trim().is_empty() && !r.correct).count();
    Ok((rows.len(), right as i32, wrong as i32))
}

#[derive(Clone)]
pub struct Graded {
    pub q: Question,
    pub selected: String,
    pub correct: bool,
    pub marks: i32,
    pub time_secs: i64,
}

pub async fn grade(pool: &SqlitePool, attempt: &str) -> anyhow::Result<(Vec<Graded>, i32, i32, i64)> {
    let paper: String = sqlx::query_scalar("SELECT paper_id FROM attempts WHERE id=?").bind(attempt).fetch_one(pool).await?;
    let qs = paper_questions(pool, &paper).await?;
    let amap = answers_map(pool, attempt).await?;
    let mut out = Vec::new();
    let (mut score, mut max, mut used) = (0i32, 0i32, 0i64);
    for q in qs {
        let (sel, _rev, secs) = amap.get(&q.id).cloned().unwrap_or_default();
        let attempted = !sel.trim().is_empty();
        let correct = is_correct(&q, &sel);
        let marks = if !attempted {
            0
        } else if correct {
            CORRECT
        } else {
            NEGATIVE
        };
        score += marks;
        max += CORRECT;
        used += secs;
        out.push(Graded { q, selected: sel, correct, marks, time_secs: secs });
    }
    Ok((out, score, max, used))
}

pub async fn finalize(pool: &SqlitePool, attempt: &str) -> anyhow::Result<(i32, i32, i64)> {
    let (rows, score, max, used) = grade(pool, attempt).await?;
    let uid: String = sqlx::query_scalar("SELECT user_id FROM attempts WHERE id=?").bind(attempt).fetch_one(pool).await?;
    sqlx::query("UPDATE attempts SET submitted_at=?, score=?, max_score=?, used_secs=? WHERE id=?")
        .bind(Utc::now().to_rfc3339())
        .bind(score)
        .bind(max)
        .bind(used)
        .bind(attempt)
        .execute(pool)
        .await?;
    for r in &rows {
        if !r.selected.trim().is_empty() {
            sqlx::query("INSERT OR IGNORE INTO seen_questions(user_id,question_id) VALUES(?,?)")
                .bind(&uid)
                .bind(r.q.id)
                .execute(pool)
                .await?;
        }
    }
    Ok((score, max, used))
}