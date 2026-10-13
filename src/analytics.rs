use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};

use crate::exam::{self, Graded};

#[derive(Clone)]
pub struct Bucket {
    pub label: String,
    pub right: i32,
    pub wrong: i32,
    pub skipped: i32,
}

impl Bucket {
    pub fn total(&self) -> i32 {
        self.right + self.wrong + self.skipped
    }
    pub fn pct(&self) -> i32 {
        if self.total() == 0 { 0 } else { (self.right as f64 / self.total() as f64 * 100.0).round() as i32 }
    }
    pub fn lost(&self) -> i32 {
        self.wrong + self.skipped
    }
    pub fn status(&self) -> &'static str {
        let t = self.total();
        if t == 0 {
            return "Good";
        }
        let lost = self.lost() as f64 / t as f64;
        if lost >= 0.5 {
            "Weak"
        } else if lost >= 0.3 {
            "Revise"
        } else {
            "Good"
        }
    }
}

#[allow(dead_code)]
pub struct Analysis {
    pub score: i32,
    pub max_score: i32,
    pub used_secs: i64,
    pub right: i32,
    pub wrong: i32,
    pub skipped: i32,
    pub by_subject: Vec<Bucket>,
    pub by_difficulty: Vec<Bucket>,
    pub by_topic: Vec<(Bucket, String)>,
    pub mistakes: Vec<Graded>,
}

pub fn accuracy(right: i32, wrong: i32) -> i32 {
    if right + wrong == 0 { 0 } else { (right as f64 / (right + wrong) as f64 * 100.0).round() as i32 }
}

pub fn analyse(rows: &[Graded], score: i32, max_score: i32, used_secs: i64) -> Analysis {
    let (mut right, mut wrong, mut skipped) = (0, 0, 0);
    let mut subj: Vec<Bucket> = Vec::new();
    let mut diff: Vec<Bucket> = Vec::new();
    let mut topic: HashMap<(String, String), Bucket> = HashMap::new();
    let mut mistakes: Vec<Graded> = Vec::new();

    for r in rows {
        let attempted = !r.selected.trim().is_empty();
        if attempted && r.correct {
            right += 1;
        } else if attempted {
            wrong += 1;
            mistakes.push((*r).clone());
        } else {
            skipped += 1;
        }
        for (list, key) in [
            (&mut subj, r.q.subject.clone()),
            (&mut diff, r.q.difficulty.clone()),
        ] {
            match list.iter_mut().find(|b| b.label == key) {
                Some(b) => {
                    if attempted && r.correct { b.right += 1 } else if attempted { b.wrong += 1 } else { b.skipped += 1 }
                }
                None => {
                    let mut b = Bucket { label: key, right: 0, wrong: 0, skipped: 0 };
                    if attempted && r.correct { b.right = 1 } else if attempted { b.wrong = 1 } else { b.skipped = 1 }
                    list.push(b);
                }
            }
        }
        let key = (r.q.chapter.clone(), r.q.subtopic.clone());
        let e = topic.entry(key).or_insert_with(|| Bucket { label: r.q.subtopic.clone(), right: 0, wrong: 0, skipped: 0 });
        if attempted && r.correct { e.right += 1 } else if attempted { e.wrong += 1 } else { e.skipped += 1 }
    }
    subj.sort_by(|a, b| a.label.cmp(&b.label));
    diff.sort_by_key(|b| match b.label.as_str() { "Easy" => 0, "Medium" => 1, _ => 2 });
    let mut by_topic: Vec<(Bucket, String)> = topic.into_iter().map(|((_, s), b)| (b, s)).collect();
    by_topic.sort_by_key(|(b, _)| (-b.lost(), b.label.clone()));

    Analysis {
        score,
        max_score,
        used_secs,
        right,
        wrong,
        skipped,
        by_subject: subj,
        by_difficulty: diff,
        by_topic,
        mistakes,
    }
}

pub struct TestRow {
    pub attempt_id: String,
    pub started_at: DateTime<Utc>,
    pub title: String,
    pub score: i32,
    pub max_score: i32,
    pub used_secs: i64,
    pub right: i32,
    pub wrong: i32,
}

pub async fn history(pool: &SqlitePool, uid: &str) -> anyhow::Result<Vec<TestRow>> {
    let rows = sqlx::query(
        "SELECT a.id, a.started_at, a.score, a.max_score, a.used_secs, a.right_count, a.wrong_count, p.title FROM attempts a
         JOIN papers p ON p.id=a.paper_id WHERE a.user_id=? AND a.submitted_at IS NOT NULL ORDER BY a.submitted_at DESC",
    )
    .bind(uid)
    .fetch_all(pool)
    .await?;
    let mut out = Vec::new();
    for r in &rows {
        let aid: String = r.get("id");
        let mut right: i32 = r.try_get("right_count").unwrap_or(0);
        let mut wrong: i32 = r.try_get("wrong_count").unwrap_or(0);
        if right == 0 && wrong == 0 && r.get::<i32, _>("score") != 0 {
            if let Ok((_, r_cnt, w_cnt)) = exam::history_counts(pool, &aid).await {
                right = r_cnt;
                wrong = w_cnt;
            }
        }
        out.push(TestRow {
            attempt_id: aid,
            started_at: r.get::<String, _>("started_at").parse().unwrap_or_else(|_| Utc::now()),
            title: r.get("title"),
            score: r.get("score"),
            max_score: r.get("max_score"),
            used_secs: r.get("used_secs"),
            right,
            wrong,
        });
    }
    Ok(out)
}

#[allow(dead_code)]
pub struct Trend {
    pub right: i32,
    pub wrong: i32,
    pub skipped: i32,
    pub seen: i32,
}

/// Mistake-tag rollup across all submitted attempts: (tag, count), most
/// common first. Tags are a fixed vocabulary enforced at write time.
pub async fn mistake_rollup(pool: &SqlitePool, uid: &str) -> Vec<(String, i64)> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT mt.tag, COUNT(*) FROM mistake_tags mt
         JOIN attempts at ON at.id=mt.attempt_id
         WHERE at.user_id=? AND at.submitted_at IS NOT NULL
         GROUP BY mt.tag ORDER BY COUNT(*) DESC",
    )
    .bind(uid)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    rows
}

pub async fn weakest(pool: &SqlitePool, uid: &str) -> anyhow::Result<Vec<(Bucket, String, String)>> {
    let rows = sqlx::query(
        "SELECT c.name AS chapter, s.name AS subtopic, q.answer, a.selected, q.qtype
         FROM answers a JOIN attempts at ON at.id=a.attempt_id JOIN paper_questions pq ON pq.paper_id=at.paper_id AND pq.question_id=a.question_id
         JOIN questions q ON q.id=a.question_id JOIN chapters c ON c.id=q.chapter_id JOIN subtopics s ON s.id=q.subtopic_id
         WHERE at.user_id=? AND at.submitted_at IS NOT NULL",
    )
    .bind(uid)
    .fetch_all(pool)
    .await?;
    let mut m: HashMap<(String, String), Bucket> = HashMap::new();
    for r in &rows {
        let sel: String = r.get("selected");
        let ans: String = r.get("answer");
        let qtype: String = r.get("qtype");
        let attempted = !sel.trim().is_empty();
        let ok = match qtype.as_str() {
            "NUM" => exam::numerical_ok(&sel, &ans),
            _ => attempted && sel.trim().eq_ignore_ascii_case(ans.trim()),
        };
        let chapter: String = r.get("chapter");
        let subtopic: String = r.get("subtopic");
        let e = m.entry((chapter, subtopic.clone())).or_insert_with(|| Bucket { label: subtopic, right: 0, wrong: 0, skipped: 0 });
        if attempted && ok { e.right += 1 } else if attempted { e.wrong += 1 } else { e.skipped += 1 }
    }
    let mut v: Vec<(Bucket, String, String)> = m.into_iter().map(|((c, s), b)| (b, c, s)).collect();
    v.sort_by_key(|(b, _, _)| (-b.lost(), b.label.clone()));
    v.truncate(8);
    Ok(v)
}

/// Human-readable label for a mistake tag key.
pub fn mistake_label(tag: &str) -> &'static str {
    match tag {
        "concept" => "concept gaps",
        "calculation" => "calculation slips",
        "misread" => "misread questions",
        "time" => "time pressure",
        "guess" => "blind guesses",
        _ => "other mistakes",
    }
}

pub fn coach_notes(
    a: &Analysis,
    history: &[TestRow],
    mistakes: &[(String, i64)],
) -> Vec<String> {
    let mut notes = Vec::new();
    if a.score == 0 && a.right == 0 {
        notes.push("No question in this paper was answered correctly. Start with the chapters you just revised, one at a time, and use unseen-only so nothing is repeated.".into());
    } else {
        notes.push(format!(
            "You scored {} of {} ({}% correct) with {} wrong and {} skipped in {}.",
            a.score,
            a.max_score,
            accuracy(a.right, a.wrong),
            a.wrong,
            a.skipped,
            fmt_dur(a.used_secs)
        ));
    }
    if let Some((w, name)) = a.by_topic.iter().find(|(b, _)| b.lost() > 0) {
        notes.push(format!("Largest mark loss: {} ({} of {} questions lost). Revise this before another full paper.", name, w.lost(), w.total()));
    }
    if let Some(h) = a.by_difficulty.iter().find(|b| b.label == "Hard") {
        if h.total() > 0 && h.pct() < 40 {
            notes.push(format!("Hard questions: {}% correct. Spend a full sitting on hard-only papers before mixing again.", h.pct()));
        }
    }
    // Dominant mistake type: the tagged reason behind most losses dictates
    // the fix (slow down vs relearn), so name it directly.
    if let Some((tag, n)) = mistakes.first() {
        let total: i64 = mistakes.iter().map(|(_, c)| c).sum::<i64>().max(1);
        if *n * 2 >= total && total >= 2 {
            let advice = match tag.as_str() {
                "calculation" => "Slow down on arithmetic and re-check each step before moving on.",
                "concept" => "Relearn the underlying chapter before attempting more papers.",
                "misread" => "Underline the numbers and the actual question asked before solving.",
                "time" => "Attempt easier questions first and return to long ones with leftover time.",
                "guess" => "Leave unknowns blank: a guess costs a mark, a blank costs nothing.",
                _ => "Review each tagged question and note the pattern.",
            };
            notes.push(format!(
                "Most of your tagged losses are {} ({} of {}). {}",
                mistake_label(tag),
                n,
                total,
                advice
            ));
        }
    }
    // Negative-marking efficiency across history: wrong answers cost a mark
    // each, so chronic guessing shows up as marks bled away.
    {
        let tw: i32 = history.iter().map(|t| t.wrong).sum();
        let tr: i32 = history.iter().map(|t| t.right).sum();
        if tw > 0 && tr + tw >= 10 && tw * 2 >= tr + tw {
            notes.push(format!(
                "Across {} papers, {} of {} attempted questions were wrong — each cost a mark. Blank the ones you cannot eliminate down to two options.",
                history.len(),
                tw,
                tr + tw
            ));
        }
    }
    if history.len() >= 2 {
        let first = history.last().unwrap();
        let last = history.first().unwrap();
        if last.max_score > 0 && first.max_score > 0 {
            let p1 = first.score as f64 / first.max_score as f64 * 100.0;
            let p2 = last.score as f64 / last.max_score as f64 * 100.0;
            notes.push(format!(
                "Across {} tests your score moved from {}% to {}%. {}",
                history.len(),
                p1.round() as i32,
                p2.round() as i32,
                if p2 >= p1 { "The trend is up." } else { "The trend is down, so the recent papers targeted something that needs another pass." }
            ));
        }
    }
    if a.skipped > a.right {
        notes.push(format!("{} questions were left blank against {} answered right. Time management, not knowledge, is costing you marks here.", a.skipped, a.right));
    }
    notes
}

/// Clock text for a countdown. An hour that is always zero takes width that the
/// minutes need on a phone, so empty leading units are dropped.
pub fn fmt_dur(s: i64) -> String {
    let (h, m, sec) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 {
        format!("{h}h {m:02}m {sec:02}s")
    } else if m > 0 {
        format!("{m}m {sec:02}s")
    } else {
        format!("{sec}s")
    }
}

pub fn fmt_pct(score: i32, max: i32) -> i32 {
    if max == 0 { 0 } else { (score as f64 / max as f64 * 100.0).round() as i32 }
}
#[cfg(test)]
mod coach_tests {
    use super::*;
    use chrono::Utc;

    fn row(score: i32, max: i32, right: i32, wrong: i32) -> TestRow {
        TestRow {
            attempt_id: "a".into(),
            started_at: Utc::now(),
            title: "t".into(),
            score,
            max_score: max,
            used_secs: 60,
            right,
            wrong,
        }
    }

    fn blank_analysis() -> Analysis {
        Analysis {
            score: 0,
            max_score: 0,
            used_secs: 0,
            right: 0,
            wrong: 0,
            skipped: 0,
            by_subject: vec![],
            by_difficulty: vec![],
            by_topic: vec![],
            mistakes: vec![],
        }
    }

    #[test]
    fn dominant_mistake_note_fires() {
        let a = blank_analysis();
        let notes = coach_notes(
            &a,
            &[],
            &[("calculation".into(), 3), ("concept".into(), 1)],
        );
        assert!(
            notes.iter().any(|n| n.contains("calculation slips")),
            "got: {notes:?}"
        );
    }

    #[test]
    fn guessing_note_fires_on_bad_history() {
        let a = blank_analysis();
        let h = vec![row(0, 40, 2, 8), row(0, 40, 2, 8)];
        let notes = coach_notes(&a, &h, &[]);
        assert!(
            notes.iter().any(|n| n.contains("Blank the ones")),
            "got: {notes:?}"
        );
    }

    #[test]
    fn no_mistake_notes_when_clean() {
        let a = blank_analysis();
        let notes = coach_notes(&a, &[], &[]);
        assert!(
            notes.iter().all(|n| !n.contains("tagged losses") && !n.contains("Blank the ones")),
            "got: {notes:?}"
        );
    }
}
