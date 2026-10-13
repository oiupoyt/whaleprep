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

/// Render question body HTML stored in the DB. The importer sanitizes at write
/// time; this re-sanitizes on read so a hand-edited row can never inject a
/// script. Only the sanitizer's allowlist survives — see
/// scripts/import_examgoal.py for the matching write-side list.
pub fn rich(s: &str) -> String {
    const ALLOWED: &[&str] = &[
        "br", "p", "sup", "sub", "b", "strong", "i", "em", "u", "ol", "ul",
        "li", "table", "tbody", "thead", "tr", "th", "td", "span", "h3",
    ];
    let mut o = String::with_capacity(s.len());
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'<' {
            // Escape text nodes so a stray "<3" or raw & cannot break markup.
            match b[i] as char {
                '&' => o.push_str("&amp;"),
                '<' => o.push_str("&lt;"),
                '>' => o.push_str("&gt;"),
                c => o.push(c),
            }
            i += 1;
            continue;
        }
        let Some(end) = s[i..].find('>') else {
            o.push_str("&lt;");
            i += 1;
            continue;
        };
        let inner = &s[i + 1..i + end];
        let inner = inner.trim();
        let (closing, name) = match inner.strip_prefix('/') {
            Some(rest) => (true, rest.split_whitespace().next().unwrap_or("")),
            None => (false, inner.split_whitespace().next().unwrap_or("")),
        };
        let name = name.trim_end_matches('/').to_ascii_lowercase();
        // Reject anything with attributes or a non-allowlisted tag. A trailing
        // slash (self-closing <br/>) is fine and needs no attribute parsing.
        let has_attrs = inner.contains(char::is_whitespace) && !inner.ends_with('/');
        if !has_attrs && ALLOWED.contains(&name.as_str()) {
            o.push('<');
            if closing {
                o.push('/');
            }
            o.push_str(&name);
            o.push('>');
        }
        i += end + 1;
    }
    o
}

pub struct ChapterBlock {
    pub id: i64,
    pub name: String,
    /// Parallel vecs: subtopic_ids[i] matches subtopics[i]
    pub subtopic_ids: Vec<i64>,
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
<title>{t} · WhalePrep</title><link rel="stylesheet" href="{b}/static/style.css">
<link rel="preconnect" href="https://fonts.googleapis.com">
<link href="https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@400;500;600&display=swap" rel="stylesheet">
<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/katex@0.16.11/dist/katex.min.css">
<script defer src="https://cdn.jsdelivr.net/npm/katex@0.16.11/dist/katex.min.js"></script>
<script defer src="https://cdn.jsdelivr.net/npm/katex@0.16.11/dist/contrib/auto-render.min.js"></script>
<script>document.addEventListener("DOMContentLoaded", function() {{
  if (window.renderMathInElement) renderMathInElement(document.body, {{
    delimiters: [{{left: "$$", right: "$$", display: true}}, {{left: "\\(", right: "\\)", display: false}}],
    trust: false, strict: false, throwOnError: false
  }});
}});</script>
</head><body>"#,
        t = esc(title),
        b = ctx.base
    )
}

fn nav(ctx: &Ctx, user: Option<&str>) -> String {
    let brand = format!(
        r#"<header class="topbar"><a class="brand" href="{b}/">WhalePrep<span>jee main practice</span></a><nav class="nav">"#,
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
    // Below 700px the row of links has nowhere to go, so the primary destinations
    // move to a bottom bar rather than disappearing behind a hidden class.
    let tabbar = match user {
        Some(_) => format!(
            r#"<nav class="tabbar" aria-label="primary"><a href="{b}/build">build</a><a href="{b}/history">history</a><a href="{b}/dashboard">dashboard</a><a href="{b}/logout">sign out</a></nav>"#,
            b = ctx.base
        ),
        None => format!(
            r#"<nav class="tabbar" aria-label="primary"><a href="{b}/">home</a><a href="{b}/build">start a paper</a><a href="{b}/terms">terms</a></nav>"#,
            b = ctx.base
        ),
    };
    format!("{brand}{links}</nav></header>{tabbar}")
}

fn foot(ctx: &Ctx) -> String {
    format!(
        r#"<footer><span>WhalePrep · JEE Main PYQ practice</span><span><a href="{b}/terms">terms</a> · <a href="{b}/privacy">privacy</a></span></footer></body></html>"#,
        b = ctx.base
    )
}

pub fn shell(ctx: &Ctx, user: Option<&str>, title: &str, body: String) -> String {
    format!("{}{}{}{}", head(ctx, title), nav(ctx, user), body, foot(ctx))
}

/// The exam screen carries a floating palette button, so the page needs extra
/// bottom room for it on a phone.
pub fn shell_exam(ctx: &Ctx, user: Option<&str>, title: &str, body: String) -> String {
    let page = shell(ctx, user, title, body);
    page.replacen("<body>", "<body class=\"exam\">", 1)
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

/// PKCE parameters for the full-page fallback: `(state, challenge, redirect uri)`.
pub type Fallback = Option<(String, String, String)>;

pub fn student(ctx: &Ctx, next: &str, err: Option<&str>, fallback: Fallback) -> String {
    let e = err.map(|m| format!(r#"<p class="bad-t" role="alert">{m}</p>"#)).unwrap_or_default();
    let google_on = ctx.google.is_some() && fallback.is_some();
    // Students are non-technical: one primary action, plain words, no jargon.
    // Google first when configured (scores follow the account across devices);
    // otherwise the name+email form, honestly labeled as this-device-only.
    let google_block = if google_on {
        format!(
            r#"<div class="signin-primary">
  <div id="google-btn" style="display:flex;justify-content:center;margin:6px 0 4px;min-height:44px"></div>
  <form method="post" action="{b}/student/google" id="google-form" hidden>
    <input type="hidden" name="credential" value="">
    <input type="hidden" name="next" value="{next}">
  </form>
  <p class="dim" id="gsi-status" role="status">loading the Google button&hellip;</p>
</div>"#,
            b = ctx.base,
            next = esc(next)
        )
    } else {
        String::new()
    };
    let divider = if google_on {
        "<div class=\"signin-or\" aria-hidden=\"true\"><span>or</span></div>".to_string()
    } else {
        String::new()
    };
    let email_head = if google_on { "use just a name and email" } else { "sign in" };
    let email_note = if google_on {
        "No Google account? Type any name and email. Your papers stay on <b>this browser only</b> — use the same email on another device and the scores won't follow."
    } else {
        "Type any name and email to start practising. Your papers stay on <b>this browser for 30 days</b> — use the same email here and everything is still waiting."
    };
    let body = format!(
        r#"<h1>Sign in</h1>
<p class="meta">One tap and your scores, mistakes and weak topics are saved.</p>
{google_block}
{divider}
<h2>{email_head}</h2>
<p class="meta">{email_note}</p>
<form method="post" action="{b}/student" style="max-width:420px;margin-top:8px">
  <input type="hidden" name="next" value="{next}">
  <label for="name">your name</label><input id="name" name="name" type="text" autocomplete="name" placeholder="e.g. Aarav Sharma" required>
  <label for="email">email</label><input id="email" name="email" type="email" autocomplete="email" placeholder="you@example.com" required>
  {e}
  <div style="margin-top:14px"><button class="btn primary block" type="submit">start practising</button></div>
</form>
<p class="dim" style="margin-top:14px;max-width:60ch">Lost access on a new phone? Sign in with the same Google account and everything returns. With the email form, scores live in this browser only — there is no password to forget because there is no password.</p>
{gsi_script}"#,
        google_block = google_block,
        divider = divider,
        email_head = email_head,
        email_note = email_note,
        b = ctx.base,
        next = esc(next),
        e = e,
        gsi_script = match &fallback {
            Some((state, challenge, redirect_uri)) if ctx.google.is_some() => {
                gsi_script(&ctx.google_client_id, state, challenge, redirect_uri)
            }
            _ => String::new(),
        }
    );
    shell(ctx, None, "Sign in", body)
}

/// Google Identity Services. The popup is the normal path; the anchor is the
/// fallback for in-app browsers and hardened settings where a popup is refused.
fn gsi_script(client_id: &str, state: &str, challenge: &str, redirect_uri: &str) -> String {
    format!(
        r#"<script src="https://accounts.google.com/gsi/client" async defer></script>
<script>
(() => {{
  const CLIENT_ID = {cid};
  const STATUS = document.getElementById('gsi-status');
  const FALLBACK = {url};

  // The popup path: Google hands the credential to this page, which posts it on.
  function onCredential(r) {{
    const f = document.getElementById('google-form');
    f.credential.value = r.credential;
    f.submit();
  }}

  function wireFallback() {{
    const a = document.getElementById('gsi-full');
    a.href = 'https://accounts.google.com/o/oauth2/v2/auth'
      + '?client_id=' + encodeURIComponent(CLIENT_ID)
      + '&redirect_uri=' + encodeURIComponent(FALLBACK.redirect_uri)
      + '&response_type=code'
      + '&scope=' + encodeURIComponent('openid email profile')
      + '&code_challenge=' + encodeURIComponent(FALLBACK.challenge)
      + '&code_challenge_method=S256'
      + '&state=' + encodeURIComponent(FALLBACK.state)
      + '&prompt=select_account';
  }}

  function init() {{
    try {{
      google.accounts.id.initialize({{ client_id: CLIENT_ID, callback: onCredential, ux_mode: 'popup', context: 'signin' }});
      const box = document.getElementById('google-btn');
      google.accounts.id.renderButton(box, {{
        theme: 'outline_black', size: 'large', text: 'signin_with', shape: 'rectangular',
        width: Math.min(box.clientWidth || 320, 400),
      }});
      wireFallback();
      STATUS.textContent = 'no popup? ';
      STATUS.appendChild(document.getElementById('gsi-full'));
    }} catch (err) {{
      wireFallback();
      STATUS.textContent = 'the Google button did not load. ';
      STATUS.appendChild(document.getElementById('gsi-full'));
    }}
  }}

  const link = document.createElement('a');
  link.id = 'gsi-full';
  link.href = '#';
  link.textContent = 'use a full page instead';
  link.addEventListener('click', e => {{ e.preventDefault(); wireFallback(); location.href = link.href; }});
  if (window.google) init();
  else window.addEventListener('load', init);
}})();
</script>"#,
        cid = serde_json::to_string(client_id).unwrap_or_else(|_| "\"\"".into()),
        url = serde_json::to_string(&serde_json::json!({ "redirect_uri": redirect_uri, "challenge": challenge, "state": state }))
            .unwrap_or_else(|_| "{}".into())
    )
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
        picker.push_str(&format!(r#"<div class="subject-section"><h3 class="subject-label"><label class="chk all-subj"><input type="checkbox" class="subj-all" data-subj="{s}"><span>{sl}</span></label></h3>"#,
            s = esc(&sb.subject), sl = esc(&sb.subject)));
        for ch in &sb.chapters {
            // Each chapter has a "select all" plus individual subtopic checkboxes.
            let subcheck: String = ch.subtopic_ids.iter().zip(ch.subtopics.iter()).map(|(sid, sname)| {
                format!(r#"<label class="chk sub-chk"><input type="checkbox" class="subtopic" value="{sid}" data-chap="{cid}"><span class="dim">{sn}</span></label>"#,
                    sid = sid, cid = ch.id, sn = esc(sname))
            }).collect();
            picker.push_str(&format!(
                r#"<div class="chapter-group">
  <label class="chk chapter-hd"><input type="checkbox" class="chap" value="{cid}" data-subj="{subj}"><span><b>{cname}</b> <span class="dim">· {count} q</span></span></label>
  <div class="subtopic-list" id="sl-{cid}">{subcheck}</div>
</div>"#,
                cid = ch.id, cname = esc(&ch.name), count = ch.count, subj = esc(&sb.subject), subcheck = subcheck
            ));
        }
        picker.push_str("</div>");
    }
    let e = err.map(|m| format!(r#"<p class="bad-t">{m}</p>"#)).unwrap_or_default();
    let body = format!(
        r#"<h1>Build a paper</h1>
<p class="meta">{available} questions in the bank. Pick chapters or individual subtopics.</p>
<form method="post" action="{b}/build" id="build-form">
  <input type="hidden" name="chapters" id="chapters" value="">
  <input type="hidden" name="subtopics" id="subtopics" value="">
  <input type="hidden" name="mode" id="mode" value="">
  <div class="grid2"><div class="picker-col">{picker}</div>
  <div class="settings-col">
    <div class="settings-sticky">
      <h2>paper settings</h2>
      <label for="count">questions</label>
      <select id="count" name="count">
        <option value="9">9 · quick drill</option>
        <option value="12" selected>12</option>
        <option value="18">18</option>
        <option value="25">25</option>
        <option value="50">50</option>
        <option value="75">75 · full shift</option>
      </select>
      <label class="chk" style="margin-top:12px">
        <input type="checkbox" id="jee-mode" name="_jee">
        <span>JEE mode — 60 min per subject<br><span class="dim">ignores the duration field</span></span>
      </label>
      <div id="dur-row">
        <label for="duration" style="margin-top:12px">duration (minutes)</label>
        <input id="duration" name="duration" type="number" value="24" min="5" max="180">
      </div>
      <label class="chk" style="margin-top:12px">
        <input type="checkbox" name="unseen_only" checked>
        <span>unseen questions only<br><span class="dim">skips anything you have already answered once</span></span>
      </label>
      <p class="dim build-note" id="pick-note" style="margin-top:10px">no topics picked yet</p>
      {e}
      <div style="margin-top:14px"><button class="btn primary block" type="submit">create paper</button></div>
    </div>
  </div></div>
</form>
<script>
(() => {{
  const form = document.getElementById('build-form');
  const chapField = document.getElementById('chapters');
  const stField = document.getElementById('subtopics');
  const modeField = document.getElementById('mode');
  const note = document.getElementById('pick-note');
  const jeeBox = document.getElementById('jee-mode');
  const durRow = document.getElementById('dur-row');

  // JEE mode hides the flat duration field.
  jeeBox.addEventListener('change', () => {{
    durRow.hidden = jeeBox.checked;
    modeField.value = jeeBox.checked ? 'jee' : '';
  }});

  // Selecting a chapter expands the subtopic list and checks all subtopics.
  document.querySelectorAll('.chap').forEach(ch => {{
    ch.addEventListener('change', () => {{
      const grp = ch.closest('.chapter-group');
      if (grp) grp.classList.toggle('expanded', ch.checked);
      document.querySelectorAll(`.subtopic[data-chap="${{ch.value}}"]`).forEach(st => {{
        st.checked = ch.checked;
      }});
      sync();
    }});
  }});

  // Selecting a "select all subjects" box checks all chapters in that subject.
  document.querySelectorAll('.subj-all').forEach(sa => {{
    sa.addEventListener('change', () => {{
      const subj = sa.dataset.subj;
      document.querySelectorAll(`.chap[data-subj="${{subj}}"]`).forEach(ch => {{
        ch.checked = sa.checked;
        document.querySelectorAll(`.subtopic[data-chap="${{ch.value}}"]`).forEach(st => {{ st.checked = sa.checked; }});
      }});
      sync();
    }});
  }});

  // A subtopic change syncs parent chapter (checked if ANY subtopic checked).
  document.querySelectorAll('.subtopic').forEach(st => {{
    st.addEventListener('change', () => {{
      const chapId = st.dataset.chap;
      const chap = document.querySelector(`.chap[value="${{chapId}}"]`);
      if (chap) {{
        const any = [...document.querySelectorAll(`.subtopic[data-chap="${{chapId}}"]`)].some(s => s.checked);
        chap.checked = any;
        chap.indeterminate = any && [...document.querySelectorAll(`.subtopic[data-chap="${{chapId}}"]`)].some(s => !s.checked);
      }}
      sync();
    }});
  }});

  function sync() {{
    const chapIds = [...document.querySelectorAll('.chap:checked')].map(c => c.value);
    const stIds   = [...document.querySelectorAll('.subtopic:checked')].map(s => s.value);
    chapField.value = chapIds.join(',');
    stField.value   = stIds.join(',');
    const n = stIds.length || chapIds.length;
    note.textContent = n ? `${{chapIds.length}} chapter${{chapIds.length!==1?'s':''}} · ${{stIds.length}} subtopics` : 'no topics picked yet';
  }}
  sync();
}})();
</script>"#,
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
                            t = rich(&text)
                        )
                    })
                    .collect()
            };
            format!(
                r#"<section class="card qpane" id="qpane-{i}" data-i="{i}"{hidden}>
  <div class="pane-head">
    <span class="dim">question {n} of {total} · {subj}</span>
    <span class="dim">{qt} · +4 / −1{revflag}</span>
  </div>
  <p class="stem">{stem}</p>
  <div class="dim meta-line">{sub2} · {ch} · {year} {shift} · {difficulty}</div>
  {opts}
  <div class="opt-actions">
    <button type="button" class="btn" data-act="prev"{prevdis}>previous</button>
    <button type="button" class="btn ok" data-act="save">save &amp; next</button>
    <button type="button" class="btn" data-act="review"{revcls}>mark for review</button>
    <button type="button" class="btn" data-act="clear">clear</button>
  </div>
</section>"#,
                hidden = if i == 0 { "" } else { " hidden" },
                prevdis = if i == 0 { " disabled" } else { "" },
                revcls = if rev { " danger" } else { "" },
                total = questions.len(),
                n = i + 1,
                subj = esc(&q.subject),
                ch = esc(&q.chapter),
                qt = esc(&q.qtype),
                revflag = if rev { " · <span class=\"warn-t\">marked</span>" } else { "" },
                stem = rich(&q.stem),
                sub2 = esc(&q.subtopic),
                year = q.year,
                shift = esc(&q.shift),
                difficulty = esc(&q.difficulty),
                opts = opts.join("")
            )
        })
        .collect();
    // Group questions by subject to build per-subject timer strips.
    let subjects_ordered: Vec<&str> = {
        let mut seen = std::collections::LinkedList::new();
        let mut added = std::collections::HashSet::new();
        for q in questions {
            if added.insert(q.subject.as_str()) { seen.push_back(q.subject.as_str()); }
        }
        seen.into_iter().collect()
    };
    // Detect JEE mode: duration == 60 * nsubjects (and nsubjects > 1).
    let is_jee = subjects_ordered.len() > 1 && duration_min == 60 * subjects_ordered.len() as i64;
    let subj_timer_bar = if is_jee {
        let strips: String = subjects_ordered.iter().enumerate().map(|(i, subj)| {
            let cls = if i == 0 { " active" } else { "" };
            let secs = 60 * 60i64; // 60 min per subject
            format!(r#"<div class="subj-timer{cls}" data-subj="{s}" data-secs="{secs}" id="st-{i}"><span class="dim">{s}</span><b>{m}:00</b></div>"#,
                cls = cls, s = esc(subj), secs = secs, i = i, m = 60)
        }).collect();
        format!(r#"<div class="subj-timer-bar" id="subj-timers">{strips}</div>"#)
    } else { String::new() };

    let body = format!(
        r#"<div class="exam-bar">
  <div class="exam-bar-inner">
    <div class="exam-title"><b>{title}</b><span class="dim" id="exam-progress">{n} questions</span></div>
    <div class="exam-clock"><span class="dim">time left</span><div class="timer" id="timer" data-secs="{secs}" role="timer">{dur}</div></div>
  </div>
  <div class="exam-progress"><i id="exam-fill"></i></div>
  {stbar}
</div>
<form method="post" action="{b}/attempt/{a}/submit" id="exam-form" data-attempt="{a}" data-base="{b}">
<div class="cbt">
  <div>{qs}</div>
  <aside class="cbt-side" id="pal-wrap">
    <h2>palette</h2>
    <div class="palette" id="pal" aria-label="question palette">{pal}</div>
    <ul class="legend">
      <li><i class="sw ans"></i>answered</li>
      <li><i class="sw rev"></i>review</li>
      <li><i class="sw skip"></i>blank</li>
    </ul>
    <p class="cbt-count dim" id="cbt-count">0 answered</p>
    <button class="btn primary block" type="submit">submit paper</button>
    <button type="button" class="btn block" data-close-palette>close palette</button>
  </aside>
</div>
<button type="button" class="btn pal-fab" id="pal-toggle" aria-expanded="false" aria-controls="pal-wrap">palette <span id="pal-fab-count"></span></button>
</form>
<script src="{b}/static/exam.js"></script>"#,
        title = esc(title),
        n = questions.len(),
        dur = fmt_dur(duration_min as i64 * 60),
        secs = duration_min * 60,
        stbar = subj_timer_bar,
        b = ctx.base,
        a = esc(attempt_id),
        qs = body_questions.join(""),
        pal = palette(questions, answers)
    );
    shell_exam(ctx, Some(user), "Test", body)
}

pub struct RowView<'a> {
    pub q: &'a Question,
    pub selected: &'a str,
    pub marks: i32,
    pub correct: bool,
    pub time_secs: i64,
    pub attempt_id: &'a str,
    pub tags: Vec<String>,
}

const MISTAKE_TAGS: &[(&str, &str)] = &[
    ("concept", "concept gap"),
    ("calculation", "calculation slip"),
    ("misread", "misread question"),
    ("time", "ran out of time"),
    ("guess", "blind guess"),
];

fn mistake_form(base: &str, attempt: &str, qid: i64, state: &str, tags: &[String]) -> String {
    if state == "correct" {
        return String::new();
    }
    let btns: String = MISTAKE_TAGS
        .iter()
        .map(|(v, label)| {
            let on = tags.iter().any(|t| t == v);
            format!(
                r#"<button type="submit" name="tag" value="{v}" class="btn sm{cls}">{label}</button>"#,
                v = v,
                cls = if on { " primary" } else { "" },
                label = label
            )
        })
        .collect::<Vec<_>>()
        .join("");
    format!(
        r#"<form method="post" action="{base}/result/{attempt}/tag/{qid}" class="mistake-row"><span class="dim">why lost?</span> {btns}</form>"#,
        base = base,
        attempt = attempt,
        qid = qid,
        btns = btns
    )
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
    let base = ctx.base.clone();
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
                        format!(r#"<span style="margin-right:10px" class="{style}">{l}: {t}{mark}</span>"#, style = style, l = l, t = rich(&r.q.opts[k]), mark = if is_ans { " (correct)" } else if is_sel { " (you)" } else { "" })
                    })
                    .collect()
            };
            format!(
                r#"<details class="card" style="margin-bottom:8px"><summary style="cursor:pointer">q{n} · {subj} · <span class="{cls}">{state}</span> · {marks:+} marks</summary>
  <p style="margin:12px 0 6px;color:#fff">{stem}</p>
  <p style="margin-bottom:8px">{opts}</p>
  <p class="dim" style="margin-bottom:8px">{sub2} · {ch} · {year} {shift} · {difficulty} · {qtime}</p>
  <p style="color:#38bdf8">{sol}</p>{mform}</details>"#,
                n = i + 1,
                subj = esc(&r.q.subject),
                cls = cls,
                state = state,
                marks = r.marks,
                stem = rich(&r.q.stem),
                opts = opts,
                sub2 = esc(&r.q.subtopic),
                ch = esc(&r.q.chapter),
                year = r.q.year,
                shift = esc(&r.q.shift),
                difficulty = esc(&r.q.difficulty),
                qtime = if r.time_secs > 0 { format!("{}s spent", r.time_secs) } else { "no time recorded".to_string() },
                sol = rich(&r.q.solution),
                mform = mistake_form(&base, r.attempt_id, r.q.id, state, &r.tags)
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
  <div class="card table-wrap"><h3>by subject</h3><table><thead><tr><th>subject</th><th>right</th><th>wrong</th><th>blank</th><th></th><th>%</th><th></th></tr></thead><tbody>{subj}</tbody></table></div>
  <div class="card table-wrap"><h3>by difficulty</h3><table><thead><tr><th>difficulty</th><th>right</th><th>wrong</th><th>blank</th><th></th><th>%</th><th></th></tr></thead><tbody>{diff}</tbody></table></div>
</div>
<h2>chapters that cost you marks</h2>
<div class="card table-wrap"><table><thead><tr><th>subtopic</th><th>right</th><th>wrong</th><th>blank</th><th></th><th>%</th><th></th></tr></thead><tbody>{topic}</tbody></table></div>
<h2>every question</h2>
{detail}
<p style="margin-top:18px;display:flex;gap:8px;flex-wrap:wrap">
  <a class="btn" href="{b}/history">all papers</a>
  <a class="btn" href="{b}/dashboard">dashboard</a>
  <button class="btn" type="button" onclick="window.print()">print / PDF</button>
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
            r#"<h1>History</h1><div class="card table-wrap" style="margin-top:16px"><table><thead><tr><th>date</th><th>paper</th><th>score</th><th>%</th><th>accuracy</th><th>time</th><th></th></tr></thead><tbody>{rows}</tbody></table></div>"#,
            rows = rows
        )
    };
    shell(ctx, Some(user), "History", body)
}

pub fn dashboard(ctx: &Ctx, user: &str, tests: &[TestRow], weak: &[(Bucket, String, String)], notes: &[String], totals: (i32, i32, i32, i32), mistakes: &[(String, i64)]) -> String {
    let (taken, marks, gained, lost) = totals;
    let trend: String = if tests.is_empty() {
        String::new()
    } else {
        let best = tests.iter().map(|t| t.score * 100 / t.max_score.max(1)).max().unwrap_or(0);
        let mut s = String::from(r#"<h2>percentage across papers</h2><div class="card table-wrap"><table><thead><tr><th>date</th><th>paper</th><th>%</th><th>accuracy</th></tr></thead><tbody>"#);
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
    let mistakes_html: String = if mistakes.is_empty() {
        String::new()
    } else {
        let total: i64 = mistakes.iter().map(|(_, n)| n).sum::<i64>().max(1);
        let rows: String = mistakes
            .iter()
            .map(|(tag, n)| {
                let label = match tag.as_str() {
                    "concept" => "concept gap",
                    "calculation" => "calculation slip",
                    "misread" => "misread question",
                    "time" => "ran out of time",
                    "guess" => "blind guess",
                    t => t,
                };
                format!(
                    r#"<tr><td>{label}</td><td>{n}</td><td style="width:38%"><div class="seg"><i style="width:{p}%;background:#38bdf8"></i></div></td><td class="dim">{p}%</td></tr>"#,
                    label = esc(label),
                    n = n,
                    p = (*n as f64 / total as f64 * 100.0).round() as i64
                )
            })
            .collect();
        format!(
            r#"<h2>why you lose marks</h2>
<div class="card table-wrap"><table><thead><tr><th>mistake type</th><th>count</th><th></th><th></th></tr></thead><tbody>{rows}</tbody></table></div>
<p class="dim" style="margin-top:8px">Tag lost questions on any result page; the most common reason surfaces here.</p>"#,
            rows = rows
        )
    };
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
<div class="card table-wrap"><table><thead><tr><th>subtopic</th><th>seen</th><th>wrong</th><th>blank</th><th></th><th></th></tr></thead><tbody>{weak}</tbody></table></div>
{mistakes_html}
<h2>coach notes</h2>
<div class="card"><ul style="padding-left:18px;display:grid;gap:6px">{notes}</ul><p class="dim" style="margin-top:10px">Written from your own submitted papers. No data leaves this machine.</p></div>
<p style="margin-top:18px"><a class="btn" href="{b}/history">all papers</a></p>"#,
            mistakes_html = mistakes_html,
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

pub fn shared(ctx: &Ctx, title: &str, duration: i64, questions: &[Question], paper_id: &str, token: &str, signed: bool) -> String {
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
                    .map(|(k, l)| format!("<p><b>{l}</b> {}</p>", rich(&q.opts[k])))
                    .collect()
            };
            format!(
                r#"<details class="card" style="margin-bottom:8px"><summary style="cursor:pointer">q{n} · {subj} · {sub2}</summary><p style="margin:12px 0 8px;color:#fff">{stem}</p>{opts}</details>"#,
                n = i + 1,
                subj = esc(&q.subject),
                sub2 = esc(&q.subtopic),
                stem = rich(&q.stem),
                opts = opts
            )
        })
        .collect();
    let cta = if signed {
        format!(
            r#"<form method="post" action="{b}/attempt/{id}" style="margin:16px 0"><button class="btn primary block" type="submit">take this paper</button></form>"#,
            b = ctx.base,
            id = esc(paper_id)
        )
    } else {
        format!(
            r#"<p style="margin:16px 0"><a class="btn primary" href="{b}/student?next=/s/{tok}">sign in to take this paper</a></p>"#,
            b = ctx.base,
            tok = esc(token)
        )
    };
    let body = format!(
        r#"<h1>{title}</h1><p class="meta">{count} questions · {dur} minutes · no answers or scores on this page.</p>{cta}<p><button class="btn" type="button" onclick="window.print()">print / PDF</button></p><div style="margin-top:16px">{qs}</div>"#,
        title = esc(title),
        count = questions.len(),
        dur = duration,
        qs = qs,
        cta = cta
    );
    shell(ctx, None, title, body)
}