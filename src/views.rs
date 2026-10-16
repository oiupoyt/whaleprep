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
        "li", "table", "tbody", "thead", "tr", "th", "td", "span",
    ];
    let mut o = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(idx) = rest.find('<') {
        let (text, after) = rest.split_at(idx);
        for c in text.chars() {
            match c {
                '&' => o.push_str("&amp;"),
                '>' => o.push_str("&gt;"),
                other => o.push(other),
            }
        }
        if let Some(end) = after.find('>') {
            let inner = after[1..end].trim();
            let (closing, name) = match inner.strip_prefix('/') {
                Some(r) => (true, r.split_whitespace().next().unwrap_or("")),
                None => (false, inner.split_whitespace().next().unwrap_or("")),
            };
            let name = name.trim_end_matches('/').to_ascii_lowercase();
            let has_attrs = inner.contains(char::is_whitespace) && !inner.ends_with('/');
            if !has_attrs && ALLOWED.contains(&name.as_str()) {
                o.push('<');
                if closing {
                    o.push('/');
                }
                o.push_str(&name);
                if !closing && inner.ends_with('/') {
                    o.push('/');
                }
                o.push('>');
            }
            rest = &after[end + 1..];
        } else {
            o.push_str("&lt;");
            rest = &after[1..];
        }
    }
    for c in rest.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '>' => o.push_str("&gt;"),
            other => o.push(other),
        }
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
        r##"<!doctype html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>{t} · WhalePrep</title>
<link rel="icon" type="image/png" href="{b}/static/favicon.png?v=2">
<link rel="apple-touch-icon" href="{b}/static/apple-touch-icon.png?v=2">
<link rel="stylesheet" href="{b}/static/style.css">
<link rel="preconnect" href="https://fonts.googleapis.com">
<link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&family=JetBrains+Mono:wght@400;500;600&display=swap" rel="stylesheet">
<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/katex@0.16.11/dist/katex.min.css">
<meta property="og:type" content="website">
<meta property="og:site_name" content="WhalePrep">
<meta property="og:title" content="{t} · WhalePrep">
<meta property="og:description" content="Practice JEE Main previous year questions on a timed exam screen. 9,813 PYQs from 2002-2025 across 53 official NTA syllabus units.">
<meta property="og:image" content="{origin}/static/embed.png">
<meta property="og:image:width" content="1200">
<meta property="og:image:height" content="630">
<meta name="twitter:card" content="summary_large_image">
<meta name="twitter:title" content="{t} · WhalePrep">
<meta name="twitter:description" content="Practice JEE Main previous year questions on a timed exam screen. 9,813 PYQs from 2002-2025.">
<meta name="twitter:image" content="{origin}/static/embed.png">
<meta name="theme-color" content="#0a0f1c">
<script defer src="https://cdn.jsdelivr.net/npm/katex@0.16.11/dist/katex.min.js"></script>
<script defer src="https://cdn.jsdelivr.net/npm/katex@0.16.11/dist/contrib/auto-render.min.js"></script>
<script>document.addEventListener("DOMContentLoaded", function() {{
  if (window.renderMathInElement) renderMathInElement(document.body, {{
    delimiters: [{{left: "$$", right: "$$", display: true}}, {{left: "\\(", right: "\\)", display: false}}],
    trust: false, strict: false, throwOnError: false
  }});
}});</script>
</head><body>"##,
        t = esc(title),
        b = ctx.base,
        origin = public_origin()
    )
}

/// The absolute origin for og:image URLs — meta tags need a full URL, not a
/// relative path. Derived from the request when behind the tunnel; falls
/// back to the production domain.
fn public_origin() -> String {
    std::env::var("PUBLIC_ORIGIN").unwrap_or_else(|_| "https://whaleprep.oiupoyt.space".into())
}

fn nav(ctx: &Ctx, user: Option<&str>) -> String {
    let brand = format!(
        r##"<header class="topbar"><div class="brand-group"><a class="brand" href="{b}/"><img class="brand-logo" src="{b}/static/logo.png" alt="WhalePrep logo" width="28" height="28"> WhalePrep</a></div><nav class="nav">"##,
        b = ctx.base
    );
    let links = match user {
        Some(u) => format!(
            r#"<a class="hidem" href="{b}/build">New paper</a><a class="hidem" href="{b}/history">My papers</a><a class="hidem" href="{b}/dashboard">Dashboard</a><span class="user-greeting">{name}</span><a class="btn-logout" href="{b}/logout">Log out</a>"#,
            b = ctx.base,
            name = esc(u)
        ),
        None => format!(
            r#"<a class="hidem" href="{b}/build">New paper</a><a class="hidem" href="{b}/terms">Terms</a><a class="btn primary" href="{b}/student">Sign in</a>"#,
            b = ctx.base
        ),
    };
    let tabbar = match user {
        Some(_) => format!(
            r#"<nav class="tabbar" aria-label="primary"><a href="{b}/build">New paper</a><a href="{b}/history">My papers</a><a href="{b}/dashboard">Dashboard</a><a href="{b}/logout">Log out</a></nav>"#,
            b = ctx.base
        ),
        None => format!(
            r#"<nav class="tabbar" aria-label="primary"><a href="{b}/">Home</a><a href="{b}/build">New paper</a><a href="{b}/student">Sign in</a></nav>"#,
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
        let subj = &sb.subject;
        picker.push_str(&format!(
            r#"<div class="zp-subject-group" data-subject="{subj}">"#,
            subj = esc(subj)
        ));
        for ch in &sb.chapters {
            let sub_count = ch.subtopics.len();
            let subcheck: String = ch.subtopic_ids.iter().zip(ch.subtopics.iter()).map(|(sid, sname)| {
                format!(
                    r#"<label class="zp-subtopic-row">
  <input type="checkbox" class="subtopic" value="{sid}" data-chap="{cid}" data-subj="{subj}">
  <span class="zp-subtopic-title">{sn}</span>
</label>"#,
                    sid = sid, cid = ch.id, subj = esc(subj), sn = esc(sname)
                )
            }).collect();

            picker.push_str(&format!(
                r#"<div class="zp-chapter-item" data-subj="{subj}" data-name="{cname_lower}">
  <div class="zp-chapter-row">
    <label class="zp-chk-wrap">
      <input type="checkbox" class="chap" value="{cid}" data-subj="{subj}" data-count="{count}">
      <span class="zp-chapter-title">{cname}</span>
    </label>
    <button type="button" class="zp-expand-btn" data-chap="{cid}" title="Toggle subtopics ({sub_count})">
      <svg class="zp-caret-icon" viewBox="0 0 24 24" width="16" height="16"><path fill="currentColor" d="M8 5v14l11-7z"/></svg>
    </button>
  </div>
  <div class="zp-subtopics-tray" id="tray-{cid}">
    <div class="zp-subtopics-tray-head">
      <span class="dim">{sub_count} subtopics in {cname}</span>
      <div style="display:flex;gap:12px">
        <button type="button" class="zp-link-btn select-all-tray" data-chap="{cid}">Select all</button>
        <button type="button" class="zp-link-btn clear-tray" data-chap="{cid}">Clear</button>
      </div>
    </div>
    <div class="zp-subtopics-list">{subcheck}</div>
  </div>
</div>"#,
                cid = ch.id,
                cname = esc(&ch.name),
                cname_lower = esc(&ch.name.to_lowercase()),
                count = ch.count,
                sub_count = sub_count,
                subj = esc(subj),
                subcheck = subcheck
            ));
        }
        picker.push_str("</div>");
    }

    let e = err.map(|m| format!(r#"<p class="bad-t" role="alert" style="margin:12px 0">{m}</p>"#)).unwrap_or_default();
    let body = format!(
        r#"<div class="zp-builder-page">
  <h1 class="zp-page-title">Prepare a JEE Main test</h1>
  <p class="zp-page-subtitle">Build a paper from the chapters you choose. {available} authentic JEE Main PYQs in the bank.</p>

  <!-- Subject Filter Pills -->
  <div class="zp-subject-pills-bar">
    <button type="button" class="zp-subj-pill active" data-subj="Maths">
      <span class="zp-dot dot-math"></span>
      <span class="zp-subj-name">Mathematics</span>
      <span class="zp-subj-badge" id="badge-math">0</span>
    </button>
    <button type="button" class="zp-subj-pill" data-subj="Physics">
      <span class="zp-dot dot-phy"></span>
      <span class="zp-subj-name">Physics</span>
      <span class="zp-subj-badge" id="badge-phy">0</span>
    </button>
    <button type="button" class="zp-subj-pill" data-subj="Chemistry">
      <span class="zp-dot dot-chem"></span>
      <span class="zp-subj-name">Chemistry</span>
      <span class="zp-subj-badge" id="badge-chem">0</span>
    </button>
  </div>

  <!-- Search and Actions Bar -->
  <div class="zp-search-row">
    <div class="zp-search-input-wrap">
      <svg class="zp-search-icon" viewBox="0 0 24 24" width="16" height="16"><path fill="none" stroke="currentColor" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"/></svg>
      <input type="text" id="zp-search-input" placeholder="Search chapters or subtopics." autocomplete="off">
    </div>
    <div class="zp-search-actions">
      <button type="button" class="zp-btn-action" id="btn-select-all">Select all</button>
      <button type="button" class="zp-btn-action" id="btn-clear-all">Clear</button>
    </div>
  </div>

  <form method="post" action="{b}/build" id="build-form">
    <input type="hidden" name="chapters" id="chapters" value="">
    <input type="hidden" name="subtopics" id="subtopics" value="">
    <input type="hidden" name="difficulties" id="difficulties" value="Easy,Medium,Hard">
    <input type="hidden" name="years" id="years" value="2020,2021,2022,2023,2024,2025">
    <input type="hidden" name="mode" id="mode" value="jee">

    <!-- Chapter Rows Container -->
    <div class="zp-chapters-container">
      {picker}
    </div>

    <!-- Bottom Filter Box (DIFFICULTY & YEARS) -->
    <div class="zp-filter-box">
      <div class="zp-filter-sec">
        <span class="zp-filter-heading">DIFFICULTY</span>
        <div class="zp-pill-row" id="diff-row">
          <label class="zp-filter-pill active">
            <input type="checkbox" class="diff-chk" value="Easy" checked>
            
            <span>Easy</span>
          </label>
          <label class="zp-filter-pill active">
            <input type="checkbox" class="diff-chk" value="Medium" checked>
            
            <span>Medium</span>
          </label>
          <label class="zp-filter-pill active">
            <input type="checkbox" class="diff-chk" value="Hard" checked>
            
            <span>Hard</span>
          </label>
        </div>
      </div>

      <div class="zp-filter-sec" style="margin-top:20px">
        <span class="zp-filter-heading">YEARS</span>
        <div class="zp-pill-row" id="year-row">
          <label class="zp-filter-pill active"><input type="checkbox" class="year-chk" value="2020" checked><span>2020</span></label>
          <label class="zp-filter-pill active"><input type="checkbox" class="year-chk" value="2021" checked><span>2021</span></label>
          <label class="zp-filter-pill active"><input type="checkbox" class="year-chk" value="2022" checked><span>2022</span></label>
          <label class="zp-filter-pill active"><input type="checkbox" class="year-chk" value="2023" checked><span>2023</span></label>
          <label class="zp-filter-pill active"><input type="checkbox" class="year-chk" value="2024" checked><span>2024</span></label>
          <label class="zp-filter-pill active"><input type="checkbox" class="year-chk" value="2025" checked><span>2025</span></label>
          <label class="zp-filter-pill"><input type="checkbox" class="year-chk" value="2019"><span>2019</span></label>
          <label class="zp-filter-pill"><input type="checkbox" class="year-chk" value="2018"><span>2018</span></label>
        </div>
      </div>
    </div>

    <!-- Paper Settings & Start Bar -->
    <div class="zp-settings-bar">
      <div class="zp-settings-row">
        <div class="zp-setting-item">
          <label class="zp-setting-label">Questions</label>
          <input type="number" id="count-input" name="count" value="30" min="5" max="75" style="width:110px">
        </div>

        <div class="zp-setting-item">
          <label class="zp-toggle-label">
            <input type="checkbox" id="jee-mode" checked>
            <span>JEE Main 60-min timer per subject</span>
          </label>
          <label class="zp-toggle-label" style="margin-top:6px">
            <input type="checkbox" name="unseen_only" checked>
            <span>Unseen questions only</span>
          </label>
        </div>

        <div class="zp-setting-item" id="dur-box" hidden>
          <label class="zp-setting-label" for="duration">Duration (Minutes)</label>
          <input type="number" id="duration" name="duration" value="60" min="5" max="180">
        </div>
      </div>

      {e}

      <div class="zp-submit-row">
        <div class="zp-summary-text" id="zp-summary">
          <b>0 chapters selected</b>
          <span class="dim" id="zp-pool-estimate">Pick chapters above</span>
        </div>
        <button type="submit" class="zp-btn-start" id="submit-btn" disabled>
          <span>Start Practice Paper</span>
          <span class="zp-btn-desc" id="btn-desc">Select topics to begin</span>
        </button>
      </div>
    </div>
  </form>
</div>

<script>
(() => {{
  const form = document.getElementById('build-form');
  const chapField = document.getElementById('chapters');
  const stField = document.getElementById('subtopics');
  const diffField = document.getElementById('difficulties');
  const yearField = document.getElementById('years');
  const modeField = document.getElementById('mode');
  const countField = document.getElementById('count-input');
  const jeeBox = document.getElementById('jee-mode');
  const durBox = document.getElementById('dur-box');
  const durInput = document.getElementById('duration');
  const submitBtn = document.getElementById('submit-btn');
  const summaryText = document.getElementById('zp-summary');
  const poolEstimate = document.getElementById('zp-pool-estimate');
  const btnDesc = document.getElementById('btn-desc');
  const searchInput = document.getElementById('zp-search-input');

  // 1. Subject Switcher Pills (Mathematics, Physics, Chemistry)
  document.querySelectorAll('.zp-subj-pill').forEach(pill => {{
    pill.addEventListener('click', () => {{
      document.querySelectorAll('.zp-subj-pill').forEach(p => p.classList.remove('active'));
      pill.classList.add('active');
      const target = pill.dataset.subj;
      document.querySelectorAll('.zp-subject-group').forEach(group => {{
        if (group.dataset.subject === target) {{
          group.style.display = 'block';
        }} else {{
          group.style.display = 'none';
        }}
      }});
      filterBySearch();
    }});
  }});

  // Initially show only Mathematics (ZenithPrep default)
  document.querySelectorAll('.zp-subject-group').forEach(g => {{
    g.style.display = g.dataset.subject === 'Maths' ? 'block' : 'none';
  }});

  // 2. Expand/Collapse Arrow buttons (▶)
  document.querySelectorAll('.zp-expand-btn').forEach(btn => {{
    btn.addEventListener('click', (e) => {{
      e.preventDefault();
      e.stopPropagation();
      const item = btn.closest('.zp-chapter-item');
      item.classList.toggle('expanded');
      btn.setAttribute('aria-expanded', item.classList.contains('expanded'));
    }});
  }});

  // 3. Search Filter
  function filterBySearch() {{
    const q = searchInput.value.trim().toLowerCase();
    const activeSubj = document.querySelector('.zp-subj-pill.active')?.dataset.subj || 'Maths';
    const activeGroup = document.querySelector(`.zp-subject-group[data-subject="${{activeSubj}}"]`);
    if (!activeGroup) return;

    activeGroup.querySelectorAll('.zp-chapter-item').forEach(item => {{
      const name = item.dataset.name || '';
      const tray = item.querySelector('.zp-subtopics-tray');
      const trayText = tray ? tray.textContent.toLowerCase() : '';
      if (!q || name.includes(q) || trayText.includes(q)) {{
        item.style.display = '';
        if (q && trayText.includes(q)) {{
          item.classList.add('expanded');
        }}
      }} else {{
        item.style.display = 'none';
      }}
    }});
  }}
  searchInput.addEventListener('input', filterBySearch);

  // 4. Select All / Clear for active subject
  document.getElementById('btn-select-all').addEventListener('click', () => {{
    const activeSubj = document.querySelector('.zp-subj-pill.active')?.dataset.subj || 'Maths';
    const activeGroup = document.querySelector(`.zp-subject-group[data-subject="${{activeSubj}}"]`);
    if (activeGroup) {{
      activeGroup.querySelectorAll('.chap, .subtopic').forEach(c => c.checked = true);
    }}
    sync();
  }});

  document.getElementById('btn-clear-all').addEventListener('click', () => {{
    const activeSubj = document.querySelector('.zp-subj-pill.active')?.dataset.subj || 'Maths';
    const activeGroup = document.querySelector(`.zp-subject-group[data-subject="${{activeSubj}}"]`);
    if (activeGroup) {{
      activeGroup.querySelectorAll('.chap, .subtopic').forEach(c => c.checked = false);
    }}
    sync();
  }});

  // 6. Subtopic Tray Select/Clear
  document.querySelectorAll('.select-all-tray').forEach(btn => {{
    btn.addEventListener('click', () => {{
      const item = btn.closest('.zp-chapter-item');
      item.querySelectorAll('.subtopic').forEach(st => st.checked = true);
      const chap = item.querySelector('.chap');
      chap.checked = true;
      chap.indeterminate = false;
      sync();
    }});
  }});

  document.querySelectorAll('.clear-tray').forEach(btn => {{
    btn.addEventListener('click', () => {{
      const item = btn.closest('.zp-chapter-item');
      item.querySelectorAll('.subtopic').forEach(st => st.checked = false);
      const chap = item.querySelector('.chap');
      chap.checked = false;
      chap.indeterminate = false;
      sync();
    }});
  }});

  // 7. Chapter and Subtopic changes
  document.querySelectorAll('.chap').forEach(ch => {{
    ch.addEventListener('change', () => {{
      const item = ch.closest('.zp-chapter-item');
      item.querySelectorAll('.subtopic').forEach(st => st.checked = ch.checked);
      ch.indeterminate = false;
      sync();
    }});
  }});

  document.querySelectorAll('.subtopic').forEach(st => {{
    st.addEventListener('change', () => {{
      const item = st.closest('.zp-chapter-item');
      const chap = item.querySelector('.chap');
      const allSubs = [...item.querySelectorAll('.subtopic')];
      const anyChecked = allSubs.some(s => s.checked);
      const allChecked = allSubs.every(s => s.checked);
      chap.checked = anyChecked;
      chap.indeterminate = anyChecked && !allChecked;
      sync();
    }});
  }});

  // 8. Difficulty and Year Pills Toggle
  document.querySelectorAll('.diff-chk, .year-chk').forEach(chk => {{
    chk.addEventListener('change', () => {{
      const pill = chk.closest('.zp-filter-pill');
      pill.classList.toggle('active', chk.checked);
      sync();
    }});
  }});

  // 9. Question count input
  countField.addEventListener('change', () => {{
    let n = parseInt(countField.value, 10);
    if (isNaN(n) || n < 5) n = 5;
    if (n > 75) n = 75;
    countField.value = n;
    sync();
  }});

  // 10. JEE Mode
  jeeBox.addEventListener('change', () => {{
    modeField.value = jeeBox.checked ? 'jee' : '';
    durBox.hidden = jeeBox.checked;
    sync();
  }});

  // 11. Sync state, pill badges, and submit button
  function sync() {{
    const checkedChaps = [...document.querySelectorAll('.chap:checked')];
    const checkedSubs = [...document.querySelectorAll('.subtopic:checked')];
    chapField.value = checkedChaps.map(c => c.value).join(',');
    stField.value = checkedSubs.map(s => s.value).join(',');

    // Difficulty & Year list
    const diffs = [...document.querySelectorAll('.diff-chk:checked')].map(c => c.value);
    const yrs = [...document.querySelectorAll('.year-chk:checked')].map(c => c.value);
    diffField.value = diffs.join(',');
    yearField.value = yrs.join(',');

    // Update subject pill badges
    ['Maths', 'Physics', 'Chemistry'].forEach(subj => {{
      const key = subj === 'Maths' ? 'math' : subj === 'Physics' ? 'phy' : 'chem';
      const badge = document.getElementById(`badge-${{key}}`);
      const cnt = document.querySelectorAll(`.chap:checked[data-subj="${{subj}}"]`).length;
      if (badge) badge.textContent = cnt;
    }});

    // Calculate pool
    let pool = 0;
    checkedChaps.forEach(c => {{
      pool += Number(c.dataset.count || 0);
    }});

    const nChapters = checkedChaps.length;
    const qCount = countField.value || 30;

    if (nChapters === 0 && checkedSubs.length === 0) {{
      summaryText.innerHTML = '<b>0 chapters selected</b><span class="dim">Pick chapters above</span>';
      submitBtn.disabled = true;
      btnDesc.textContent = 'Select topics to begin';
    }} else {{
      summaryText.innerHTML = `<b>${{nChapters}} chapters (${{checkedSubs.length}} subtopics)</b><span class="dim">~${{pool.toLocaleString()}} questions in pool</span>`;
      submitBtn.disabled = false;
      const subjsCount = new Set(checkedChaps.map(c => c.dataset.subj)).size;
      const duration = jeeBox.checked ? `${{Math.max(1, subjsCount) * 60}} Mins (JEE Mode)` : `${{durInput.value}} Mins`;
      btnDesc.textContent = `${{qCount}} Questions · ${{duration}}`;
    }}
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
                            r#"<label class="opt{cls}"><input type="radio" name="q-{id}" value="{letter}"{checked}><span class="opt-letter">{letter}</span><span class="opt-text">{t}</span></label>"#,
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
  <div class="stem">{stem}</div>
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
  <div class="stem" style="margin:12px 0 6px">{stem}</div>
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
                r#"<details class="card" style="margin-bottom:8px"><summary style="cursor:pointer">q{n} · {subj} · {sub2}</summary><div class="stem" style="margin:12px 0 8px">{stem}</div>{opts}</details>"#,
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