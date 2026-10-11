use crate::analytics::{accuracy, fmt_dur, fmt_pct, Analysis, Bucket, TestRow};
use crate::auth::Ctx;
use crate::exam::{BuildForm, Question};

pub fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 16);
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&#39;"),
            _ => o.push(c),
        }
    }
    o
}

pub struct ChapterBlock {
    pub id: i64,
    pub name: String,
    pub subtopics: Vec<String>,
    pub count: i64,
}

pub struct SubjectBlock {
    pub subject: String,
    pub chapters: Vec<ChapterBlock>,
}

fn head(ctx: &Ctx, title: &str) -> String {
    format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>{t} · PYQ</title><link rel="stylesheet" href="{b}/static/style.css">
<link rel="preconnect" href="https://fonts.googleapis.com">
<link href="https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@400;500;600&display=swap" rel="stylesheet">
</head><body>"#,
        t = esc(title),
        b = ctx.base
    )
}

fn nav(ctx: &Ctx, user: Option<&str>) -> String {
    let brand = format!(
        r#"<header class="topbar"><a class="brand" href="{b}/">PYQ<span>jee main practice</span></a><nav class="nav">"#,
        b = ctx.base
    );
    let links = match user {
        Some(_) => format!(
            r#"<a class="hidem" href="{b}/build">build</a><a class="hidem" href="{b}/dashboard">dashboard</a><a class="hidem" href="{b}/history">history</a><a class="btn" href="{b}/logout">sign out</a>"#,
            b = ctx.base
        ),
        None => format!(
            r#"<a class="hidem" href="{b}/terms">terms</a><a class="hidem" href="{b}/privacy">privacy</a><a class="btn primary" href="{b}/build">start a paper</a>"#,
            b = ctx.base
        ),
    };
    format!("{brand}{links}</nav></header>")
}

fn foot(ctx: &Ctx) -> String {
    format!(
        r#"<footer><span>PYQ clone · running on sqlite · built as a working copy of zenithprepare.com/PYQ</span><span><a href="{b}/terms">terms</a> · <a href="{b}/privacy">privacy</a></span></footer></body></html>"#,
        b = ctx.base
    )
}

pub fn shell(ctx: &Ctx, user: Option<&str>, title: &str, body: String) -> String {
    format!("{}{}{}{}", head(ctx, title), nav(ctx, user), body, foot(ctx))
}

pub fn landing(ctx: &Ctx, q: i64, shifts: i64, subtopics: i64, subjects: i64, students: i64, signed: bool) -> String {
    let cta = if signed {
        format!(r#"<a class="btn primary" href="{b}/build">build a paper</a>"#, b = ctx.base)
    } else {
        format!(r#"<a class="btn primary" href="{b}/build">start a paper</a>"#, b = ctx.base)
    };
    let body = format!(
        r#"<h1>JEE Main practice, built like the real exam screen.</h1>
<p class="meta">Pick the chapters you just revised. Take the paper on a timed computer-based screen with the +4 / −1 marking scheme. Get the analysis the moment you submit.</p>
<div class="kv" style="margin-top:22px">
  <div class="card"><span class="dim">questions</span><b>{q}</b></div>
  <div class="card"><span class="dim">shifts</span><b>{shifts}</b></div>
  <div class="card"><span class="dim">subtopics</span><b>{subtopics}</b></div>
  <div class="card"><span class="dim">students</span><b>{students}</b></div>
</div>
<div class="grid2" style="margin-top:18px">
  <div class="card"><h3>Build</h3><p class="meta">Choose any mix of chapters across {subjects} subjects. Questions are drawn from real shift data and, with unseen-only on, you never repeat one by accident.</p></div>
  <div class="card"><h3>Attempt</h3><p class="meta">Subject tabs, a colour-coded question palette, mark for review, numerical answers, a countdown timer, and +4 for a correct answer with −1 for a wrong one.</p></div>
  <div class="card"><h3>Analysis</h3><p class="meta">Score, accuracy, time per question, and results broken down by subject, chapter and difficulty. Solutions sit under every question.</p></div>
  <div class="card"><h3>Progress</h3><p class="meta">Every submitted paper is kept. The dashboard tracks percentage and accuracy across tests and names the subtopics costing you the most marks.</p></div>
</div>
<p style="margin-top:20px">{cta}</p>"#,
        q = q,
        shifts = shifts,
        subtopics = subtopics,
        subjects = subjects,
        students = students,
        cta = cta
    );
    shell(ctx, None, "Home", body)
}

pub fn student(ctx: &Ctx, next: &str, err: Option<&str>) -> String {
    let e = err.map(|m| format!(r#"<p class="bad-t">{m}</p>"#)).unwrap_or_default();
    let body = format!(
        r#"<h1>Sign in</h1>
<p class="meta">This clone runs a local sign-in so you can use the whole flow without a Google account. Enter any name and email; your papers stay in this database.</p>
<form method="post" action="{b}/student" style="max-width:420px;margin-top:18px">
  <input type="hidden" name="next" value="{next}">
  <label for="name">name</label><input id="name" name="name" type="text" autocomplete="name" required>
  <label for="email">email</label><input id="email" name="email" type="email" autocomplete="email" required>
  {e}
  <div style="margin-top:14px"><button class="btn primary" type="submit">sign in</button></div>
</form>"#,
        b = ctx.base,
        next = esc(next),
        e = e
    );
    shell(ctx, None, "Sign in", body)
}

pub fn legal(ctx: &Ctx, title: &str, body_md: &str) -> String {
    let body = format!(r#"<h1>{t}</h1><div style="max-width:70ch">{b}</div>"#, t = esc(title), b = body_md);
    shell(ctx, None, title, body)
}

pub fn error(ctx: &Ctx, code: u16, detail: &str) -> String {
    let (t, d) = match code {
        404 => ("Nothing at this address".to_string(), "The link may be wrong, or the paper was never created.".to_string()),
        _ => ("Something broke on our side".to_string(), detail.to_string()),
    };
    let body = format!(
        r#"<h1>{t}</h1><p class="meta">{d}</p><p style="margin-top:16px"><a class="btn" href="{b}/">back to home</a></p>"#,
        t = esc(&t),
        d = esc(&d),
        b = ctx.base
    );
    shell(ctx, None, &t, body)
}

pub fn build(ctx: &Ctx, user: &str, blocks: Vec<SubjectBlock>, _form: &BuildForm, available: i64, err: Option<&str>) -> String {
    let mut picker = String::new();
    for sb in &blocks {
        picker.push_str(&format!("<h3>{}</h3>", esc(&sb.subject)));
        for ch in &sb.chapters {
            let subs = ch.subtopics.iter().map(|s| esc(s)).collect::<Vec<_>>().join(", ");
            picker.push_str(&format!(
                r#"<label class="chk"><input type="checkbox" class="chap" value="{cid}"><span><b>{cname}</b><br><span class="dim">{subs} · {count} questions</span></span></label>"#,
                cid = ch.id,
                cname = esc(&ch.name),
                subs = subs,
                count = ch.count
            ));
        }
    }
    let e = err.map(|m| format!(r#"<p class="bad-t">{m}</p>"#)).unwrap_or_default();
    let body = format!(
        r#"<h1>Build a paper</h1>
<p class="meta">{available} questions match this bank. Pick the chapters you are revising; the paper is drawn from them and marked on the +4 / −1 scheme.</p>
<form method="post" action="{b}/build" style="margin-top:18px">
  <input type="hidden" name="chapters" id="chapters" value="">
  <div class="grid2"><div>{picker}</div>
  <div>
    <div style="position:sticky;top:64px">
      <h2>paper settings</h2>
      <label for="count">questions</label>
      <select id="count" name="count">
        <option value="9">9 · short drill</option><option value="12" selected>12</option>
        <option value="18">18</option><option value="25">25</option><option value="36">36 · full paper</option>
      </select>
      <label for="duration">duration in minutes</label>
      <input id="duration" name="duration" type="number" value="24" min="5" max="180">
      <label class="chk" style="margin-top:14px"><input type="checkbox" name="unseen_only" checked><span>unseen questions only<br><span class="dim">skips anything you have already answered once</span></span></label>
      <p class="dim" id="pick-note" style="margin-top:10px">no chapters picked yet</p>
      {e}
      <div style="margin-top:14px"><button class="btn primary" type="submit">create paper</button></div>
    </div>
  </div></div>
</form>
<script>// Repeated checkbox keys do not survive a urlencoded body, so the ids are joined into one field.
(() => {{
  const boxes = [...document.querySelectorAll('.chap')];
  const field = document.getElementById('chapters');
  const note = document.getElementById('pick-note');
  const sync = () => {{
    field.value = boxes.filter(b => b.checked).map(b => b.value).join(',');
    note.textContent = field.value ? field.value.split(',').length + ' chapters picked' : 'no chapters picked yet';
  }};
  boxes.forEach(b => b.addEventListener('change', sync));
  sync();
}})();</script>"#,
        available = available,
        b = ctx.base,
        picker = picker,
        e = e
    );
    shell(ctx, Some(user), "Build", body)
}

fn palette(questions: &[Question], answers: &[(i64, String, bool)]) -> String {
    questions
        .iter()
        .enumerate()
        .map(|(i, q)| {
            let a = answers.iter().find(|(id, _, _)| *id == q.id);
            let sel = a.map(|(_, s, _)| !s.trim().is_empty()).unwrap_or(false);
            let rev = a.map(|(_, _, r)| *r).unwrap_or(false);
            let cls = match (sel, rev) {
                (true, true) => "pal rev",
                (true, false) => "pal ans",
                _ => "pal",
            };
            format!(r#"<button type="button" class="{cls}" data-jump="{i}">{n}</button>"#, cls = cls, n = i + 1)
        })
        .collect::<Vec<_>>()
        .join("")
}

pub fn attempt(ctx: &Ctx, user: &str, attempt_id: &str, questions: &[Question], answers: &[(i64, String, bool)], duration_min: i64, title: &str) -> String {
    let body_questions: Vec<String> = questions
        .iter()
        .enumerate()
        .map(|(i, q)| {
            let cur = answers.iter().find(|(id, _, _)| *id == q.id);
            let sel = cur.map(|(_, s, _)| s.clone()).unwrap_or_default();
            let rev = cur.map(|(_, _, r)| *r).unwrap_or(false);
            let opts: Vec<String> = if q.qtype == "NUM" {
                vec![format!(
                    r#"<input type="text" inputmode="decimal" name="num-{id}" value="{v}" placeholder="numeric answer">"#,
                    id = q.id,
                    v = esc(&sel)
                )]
            } else {
                ["A", "B", "C", "D"]
                    .iter()
                    .enumerate()
                    .map(|(k, letter)| {
                        let text = q.opts[k].clone();
                        let checked = if sel.trim().eq_ignore_ascii_case(letter) { " checked" } else { "" };
                        format!(
                            r#"<label class="opt{cls}"><input type="radio" name="q-{id}" value="{letter}"{checked}><span><b>{letter}</b> &nbsp;{t}</span></label>"#,
                            cls = if sel.trim().eq_ignore_ascii_case(letter) { " sel" } else { "" },
                            id = q.id,
                            letter = letter,
                            checked = checked,
                            t = esc(&text)
                        )
                    })
                    .collect()
            };
            format!(
                r#"<section class="card qpane" id="qpane-{i}" data-i="{i}"{hidden}>
  <div style="display:flex;justify-content:space-between;align-items:center;gap:10px;flex-wrap:wrap">
    <span class="dim">question {n} · {subj} · {ch}</span>
    <span class="dim">{qt} · +4 / −1{revflag}</span>
  </div>
  <p style="margin:10px 0 4px;color:#fff">{stem}</p>
  <div class="dim" style="margin-bottom:8px">{sub2} · {year} {shift} · {difficulty}</div>
  {opts}
  <div style="display:flex;gap:8px;flex-wrap:wrap;margin-top:12px">
    <button type="button" class="btn ok" data-act="save">save &amp; next</button>
    <button type="button" class="btn" data-act="review">mark for review</button>
    <button type="button" class="btn" data-act="clear">clear</button>
  </div>
</section>"#,
                hidden = if i == 0 { "" } else { " hidden" },
                n = i + 1,
                subj = esc(&q.subject),
                ch = esc(&q.chapter),
                qt = esc(&q.qtype),
                revflag = if rev { " · <span class=\"warn-t\">marked</span>" } else { "" },
                stem = esc(&q.stem),
                sub2 = esc(&q.subtopic),
                year = q.year,
                shift = esc(&q.shift),
                difficulty = esc(&q.difficulty),
                opts = opts.join("")
            )
        })
        .collect();
    let body = format!(
        r#"<div style="display:flex;justify-content:space-between;align-items:center;gap:12px;flex-wrap:wrap;margin:14px 0">
  <div><h1 style="margin:0">{title}</h1><span class="dim">{n} questions · +4 / −1</span></div>
  <div style="text-align:right"><div class="timer" id="timer" data-secs="{secs}">{dur}</div><div class="dim">remaining</div></div>
</div>
<form method="post" action="{b}/attempt/{a}/submit" id="exam-form" data-attempt="{a}">
<div class="cbt">
  <div>{qs}</div>
  <div><h2>palette</h2><div class="palette" id="pal">{pal}</div>
    <p class="dim" style="margin-top:10px"><span class="ok-t">answered</span> · <span class="warn-t">review</span> · <span class="dim">blank</span></p>
    <button class="btn primary" type="submit" style="width:100%;margin-top:12px">submit paper</button>
  </div>
</div></form>
<script src="{b}/static/exam.js"></script>"#,
        title = esc(title),
        n = questions.len(),
        dur = fmt_dur(duration_min as i64 * 60),
        secs = duration_min * 60,
        b = ctx.base,
        a = esc(attempt_id),
        qs = body_questions.join(""),
        pal = palette(questions, answers)
    );
    shell(ctx, Some(user), "Test", body)
}

pub struct RowView<'a> {
    pub q: &'a Question,
    pub selected: &'a str,
    pub marks: i32,
    pub correct: bool,
    pub time_secs: i64,
}

fn seg(b: &Bucket) -> String {
    let t = b.total().max(1) as f64;
    format!(
        r#"<div class="seg"><i style="width:{}%;background:#10b981"></i><i style="width:{}%;background:#ff5555"></i><i style="width:{}%;background:#333"></i></div>"#,
        b.right as f64 / t * 100.0,
        b.wrong as f64 / t * 100.0,
        b.skipped as f64 / t * 100.0
    )
}

fn bucket_rows(items: &[Bucket]) -> String {
    items
        .iter()
        .map(|b| {
            format!(
                r#"<tr><td>{l}</td><td>{r}</td><td>{w}</td><td>{s}</td><td style="width:38%">{seg}</td><td>{p}%</td><td class="{st}">{st}</td></tr>"#,
                l = esc(&b.label),
                r = b.right,
                w = b.wrong,
                s = b.skipped,
                seg = seg(b),
                p = b.pct(),
                st = b.status()
            )
        })
        .collect()
}

pub fn result(ctx: &Ctx, user: &str, _attempt_id: &str, a: &Analysis, rows: &[RowView]) -> String {
    let detail: String = rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let state = if r.correct { "correct" } else if r.selected.trim().is_empty() { "blank" } else { "wrong" };
            let cls = match state { "correct" => "ok-t", "wrong" => "bad-t", _ => "dim" };
            let opts: String = if r.q.qtype == "NUM" {
                format!(r#"<p class="dim">your answer: {sel} · correct: {ans}</p>"#, sel = esc(if r.selected.is_empty() { "not attempted" } else { r.selected }), ans = esc(&r.q.answer))
            } else {
                ["A", "B", "C", "D"]
                    .iter()
                    .enumerate()
                    .map(|(k, l)| {
                        let is_ans = r.q.answer.trim().eq_ignore_ascii_case(l);
                        let is_sel = r.selected.trim().eq_ignore_ascii_case(l);
                        let style = if is_ans { "ok-t" } else if is_sel { "bad-t" } else { "dim" };
                        format!(r#"<span style="margin-right:10px" class="{style}">{l}: {t}{mark}</span>"#, style = style, l = l, t = esc(&r.q.opts[k]), mark = if is_ans { " (correct)" } else if is_sel { " (you)" } else { "" })
                    })
                    .collect()
            };
            format!(
                r#"<details class="card" style="margin-bottom:8px"><summary style="cursor:pointer">q{n} · {subj} · <span class="{cls}">{state}</span> · {marks:+} marks</summary>
  <p style="margin:12px 0 6px;color:#fff">{stem}</p>
  <p style="margin-bottom:8px">{opts}</p>
  <p class="dim" style="margin-bottom:8px">{sub2} · {ch} · {year} {shift} · {difficulty} · {qtime}</p>
  <p style="color:#38bdf8">{sol}</p></details>"#,
                n = i + 1,
                subj = esc(&r.q.subject),
                cls = cls,
                state = state,
                marks = r.marks,
                stem = esc(&r.q.stem),
                opts = opts,
                sub2 = esc(&r.q.subtopic),
                ch = esc(&r.q.chapter),
                year = r.q.year,
                shift = esc(&r.q.shift),
                difficulty = esc(&r.q.difficulty),
                qtime = if r.time_secs > 0 { format!("{}s spent", r.time_secs) } else { "no time recorded".to_string() },
                sol = esc(&r.q.solution)
            )
        })
        .collect();
    let topic_buckets: Vec<Bucket> = a.by_topic.iter().map(|(b, _)| b.to_owned()).collect();
    let body = format!(
        r#"<h1>Result</h1>
<div class="kv">
  <div class="card"><span class="dim">score</span><b>{score} <span class="dim">/ {max}</span></b></div>
  <div class="card"><span class="dim">percentage</span><b>{pct}%</b></div>
  <div class="card"><span class="dim">accuracy</span><b class="ok-t">{acc}%</b></div>
  <div class="card"><span class="dim">time used</span><b>{dur}</b></div>
</div>
<div class="grid2">
  <div class="card"><h3>by subject</h3><table><thead><tr><th>subject</th><th>right</th><th>wrong</th><th>blank</th><th></th><th>%</th><th></th></tr></thead><tbody>{subj}</tbody></table></div>
  <div class="card"><h3>by difficulty</h3><table><thead><tr><th>difficulty</th><th>right</th><th>wrong</th><th>blank</th><th></th><th>%</th><th></th></tr></thead><tbody>{diff}</tbody></table></div>
</div>
<h2>chapters that cost you marks</h2>
<div class="card"><table><thead><tr><th>subtopic</th><th>right</th><th>wrong</th><th>blank</th><th></th><th>%</th><th></th></tr></thead><tbody>{topic}</tbody></table></div>
<h2>every question</h2>
{detail}
<p style="margin-top:18px;display:flex;gap:8px;flex-wrap:wrap">
  <a class="btn" href="{b}/history">all papers</a>
  <a class="btn" href="{b}/dashboard">dashboard</a>
  <a class="btn primary" href="{b}/build">new paper</a>
</p>"#,
        score = a.score,
        max = a.max_score,
        pct = fmt_pct(a.score, a.max_score),
        acc = accuracy(a.right, a.wrong),
        dur = fmt_dur(a.used_secs),
        subj = bucket_rows(&a.by_subject),
        diff = bucket_rows(&a.by_difficulty),
        topic = bucket_rows(&topic_buckets),
        detail = detail,
        b = ctx.base
    );
    shell(ctx, Some(user), "Result", body)
}

pub fn history(ctx: &Ctx, user: &str, tests: &[TestRow]) -> String {
    let rows: String = if tests.is_empty() {
        String::new()
    } else {
        tests
            .iter()
            .map(|t| {
                format!(
                    r#"<tr><td>{d}</td><td>{title}</td><td>{score} / {max}</td><td>{p}%</td><td>{acc}%</td><td>{dur}</td><td><a href="{b}/result/{id}">open</a></td></tr>"#,
                    d = t.started_at.format("%d %b %Y"),
                    title = esc(&t.title),
                    score = t.score,
                    max = t.max_score,
                    p = fmt_pct(t.score, t.max_score),
                    acc = accuracy(t.right, t.wrong),
                    dur = fmt_dur(t.used_secs),
                    b = ctx.base,
                    id = esc(&t.attempt_id)
                )
            })
            .collect()
    };
    let body = if tests.is_empty() {
        format!(
            r#"<h1>History</h1><div class="card" style="margin-top:16px;max-width:60ch"><h3>No papers yet</h3><p class="meta">Nothing is here because no paper has been submitted. Build one and it appears here with its score and accuracy.</p><p style="margin-top:12px"><a class="btn primary" href="{b}/build">build your first paper</a></p></div>"#,
            b = ctx.base
        )
    } else {
        format!(
            r#"<h1>History</h1><div class="card" style="margin-top:16px;overflow-x:auto"><table><thead><tr><th>date</th><th>paper</th><th>score</th><th>%</th><th>accuracy</th><th>time</th><th></th></tr></thead><tbody>{rows}</tbody></table></div>"#,
            rows = rows
        )
    };
    shell(ctx, Some(user), "History", body)
}

pub fn dashboard(ctx: &Ctx, user: &str, tests: &[TestRow], weak: &[(Bucket, String, String)], notes: &[String], totals: (i32, i32, i32, i32)) -> String {
    let (taken, marks, gained, lost) = totals;
    let trend: String = if tests.is_empty() {
        String::new()
    } else {
        let best = tests.iter().map(|t| t.score * 100 / t.max_score.max(1)).max().unwrap_or(0);
        let mut s = String::from(r#"<h2>percentage across papers</h2><div class="card"><table><thead><tr><th>date</th><th>paper</th><th>%</th><th>accuracy</th></tr></thead><tbody>"#);
        for t in tests {
            s.push_str(&format!(
                r#"<tr><td>{}</td><td>{}</td><td>{}%</td><td>{}%</td></tr>"#,
                t.started_at.format("%d %b"),
                esc(&t.title),
                fmt_pct(t.score, t.max_score),
                accuracy(t.right, t.wrong)
            ));
        }
        s.push_str("</tbody></table></div>");
        s = s.replace("{best}", &best.to_string());
        s
    };
    let weak_rows: String = weak
        .iter()
        .map(|(b, chapter, subtopic)| {
            format!(
                r#"<tr><td>{sub} <span class="dim">· {ch}</span></td><td>{t}</td><td>{w}</td><td>{s}</td><td>{seg}</td><td class="{st}">{st}</td></tr>"#,
                sub = esc(subtopic),
                ch = esc(chapter),
                t = b.total(),
                w = b.wrong,
                s = b.skipped,
                seg = seg(b),
                st = b.status()
            )
        })
        .collect();
    let notes_html: String = notes.iter().map(|n| format!("<li>{}</li>", esc(n))).collect();
    let body = if tests.is_empty() {
        format!(
            r#"<h1>Dashboard</h1><div class="card" style="margin-top:16px;max-width:60ch"><h3>No test data yet</h3><p class="meta">The trend, the weak-subtopic table and the coach notes all come from papers you submit, so there is nothing to show until the first one is done.</p><p style="margin-top:12px"><a class="btn primary" href="{b}/build">build a paper</a></p></div>"#,
            b = ctx.base
        )
    } else {
        format!(
            r#"<h1>Dashboard</h1>
<div class="kv">
  <div class="card"><span class="dim">papers submitted</span><b>{taken}</b></div>
  <div class="card"><span class="dim">marks scored</span><b>{marks}</b></div>
  <div class="card"><span class="dim">correct</span><b class="ok-t">{gained}</b></div>
  <div class="card"><span class="dim">wrong</span><b class="bad-t">{lost}</b></div>
</div>
{trend}
<h2>weakest subtopics across all papers</h2>
<div class="card"><table><thead><tr><th>subtopic</th><th>seen</th><th>wrong</th><th>blank</th><th></th><th></th></tr></thead><tbody>{weak}</tbody></table></div>
<h2>coach notes</h2>
<div class="card"><ul style="padding-left:18px;display:grid;gap:6px">{notes}</ul><p class="dim" style="margin-top:10px">Written from your own submitted papers. No data leaves this machine.</p></div>
<p style="margin-top:18px"><a class="btn" href="{b}/history">all papers</a></p>"#,
            taken = taken,
            marks = marks,
            gained = gained,
            lost = lost,
            trend = trend,
            weak = weak_rows,
            notes = notes_html,
            b = ctx.base
        )
    };
    shell(ctx, Some(user), "Dashboard", body)
}

pub fn shared(ctx: &Ctx, title: &str, duration: i64, questions: &[Question]) -> String {
    let qs: String = questions
        .iter()
        .enumerate()
        .map(|(i, q)| {
            let opts = if q.qtype == "NUM" {
                String::new()
            } else {
                ["A", "B", "C", "D"]
                    .iter()
                    .enumerate()
                    .map(|(k, l)| format!("<p><b>{l}</b> {}</p>", esc(&q.opts[k])))
                    .collect()
            };
            format!(
                r#"<details class="card" style="margin-bottom:8px"><summary style="cursor:pointer">q{n} · {subj} · {sub2}</summary><p style="margin:12px 0 8px;color:#fff">{stem}</p>{opts}</details>"#,
                n = i + 1,
                subj = esc(&q.subject),
                sub2 = esc(&q.subtopic),
                stem = esc(&q.stem),
                opts = opts
            )
        })
        .collect();
    let body = format!(
        r#"<h1>{title}</h1><p class="meta">{count} questions · {dur} minutes · no answers or scores on this page.</p><div style="margin-top:16px">{qs}</div>"#,
        title = esc(title),
        count = questions.len(),
        dur = duration,
        qs = qs
    );
    shell(ctx, None, title, body)
}