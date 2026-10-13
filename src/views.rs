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
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>{t} · WhalePrep</title><link rel="stylesheet" href="{b}/static/style.css">
<link rel="preconnect" href="https://fonts.googleapis.com">
<link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&family=JetBrains+Mono:wght@400;500;600&display=swap" rel="stylesheet">
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
        let _subj_lower = sb.subject.to_lowercase();
        picker.push_str(&format!(
            r#"<div class="subject-section" data-subject="{subj}">
  <div class="subject-header-row">
    <span class="subject-title-badge">{subj}</span>
    <button type="button" class="btn-select-all-subject" data-subj="{subj}">Select all {subj}</button>
  </div>"#,
            subj = esc(&sb.subject)
        ));
        for ch in &sb.chapters {
            let sub_count = ch.subtopics.len();
            let subcheck: String = ch.subtopic_ids.iter().zip(ch.subtopics.iter()).map(|(sid, sname)| {
                format!(
                    r#"<label class="subtopic-chip">
  <input type="checkbox" class="subtopic" value="{sid}" data-chap="{cid}" data-subj="{subj}">
  <span>{sn}</span>
</label>"#,
                    sid = sid, cid = ch.id, subj = esc(&sb.subject), sn = esc(sname)
                )
            }).collect();

            picker.push_str(&format!(
                r#"<div class="chapter-card" data-subj="{subj}" data-chap="{cid}">
  <div class="chapter-card-header">
    <label class="chapter-select-wrap">
      <input type="checkbox" class="chap" value="{cid}" data-subj="{subj}" data-count="{count}">
      <span class="chapter-name">{cname}</span>
    </label>
    <div class="chapter-actions">
      <span class="chapter-q-badge">{count} Qs</span>
      <button type="button" class="btn-expand-subtopics" data-chap="{cid}" title="Toggle subtopics list">
        <span>{sub_count} Subtopics</span>
        <svg class="chevron-icon" viewBox="0 0 24 24" width="16" height="16"><path fill="none" stroke="currentColor" stroke-width="2" d="M6 9l6 6 6-6"/></svg>
      </button>
    </div>
  </div>
  <div class="subtopics-tray" id="tray-{cid}">
    <div class="subtopics-tray-head">
      <span class="dim">Subtopics in {cname}</span>
      <div style="display:flex;gap:12px">
        <button type="button" class="btn-sub-link select-all-tray" data-chap="{cid}">Select all</button>
        <button type="button" class="btn-sub-link clear-tray" data-chap="{cid}">Deselect all</button>
      </div>
    </div>
    <div class="subtopics-grid">{subcheck}</div>
  </div>
</div>"#,
                cid = ch.id,
                cname = esc(&ch.name),
                count = ch.count,
                sub_count = sub_count,
                subj = esc(&sb.subject),
                subcheck = subcheck
            ));
        }
        picker.push_str("</div>");
    }

    let e = err.map(|m| format!(r#"<p class="bad-t" role="alert">{m}</p>"#)).unwrap_or_default();
    let body = format!(
        r#"<div class="build-header">
  <h1>Build Practice Paper</h1>
  <p class="meta">Select specific syllabus units or choose a preset from <b>{available}</b> authentic JEE Main questions.</p>
  
  <div class="preset-bar">
    <span class="preset-label">Quick Presets:</span>
    <button type="button" class="preset-btn" data-preset="full">⚡ Full Mock (75 Qs · All 3 Subjects)</button>
    <button type="button" class="preset-btn" data-preset="phy">⚛ Physics Focus</button>
    <button type="button" class="preset-btn" data-preset="chem">🧪 Chemistry Focus</button>
    <button type="button" class="preset-btn" data-preset="math">📐 Mathematics Focus</button>
    <button type="button" class="preset-btn secondary" data-preset="clear">✕ Clear Selection</button>
  </div>
</div>

<div class="subject-tabs" role="tablist">
  <button type="button" class="sub-tab active" data-tab="all">All Subjects <span class="tab-badge">15 Units</span></button>
  <button type="button" class="sub-tab" data-tab="Physics">Physics <span class="tab-badge">6</span></button>
  <button type="button" class="sub-tab" data-tab="Chemistry">Chemistry <span class="tab-badge">3</span></button>
  <button type="button" class="sub-tab" data-tab="Maths">Mathematics <span class="tab-badge">6</span></button>
</div>

<form method="post" action="{b}/build" id="build-form">
  <input type="hidden" name="chapters" id="chapters" value="">
  <input type="hidden" name="subtopics" id="subtopics" value="">
  <input type="hidden" name="mode" id="mode" value="">

  <div class="build-layout">
    <div class="picker-col">
      {picker}
    </div>

    <div class="settings-col">
      <div class="settings-sticky">
        <div class="settings-card">
          <h2 class="settings-title">Exam Settings</h2>

          <div class="setting-group">
            <label class="setting-lbl">Questions Count</label>
            <div class="count-pills">
              <button type="button" class="count-pill" data-count="9">9</button>
              <button type="button" class="count-pill active" data-count="15">15</button>
              <button type="button" class="count-pill" data-count="30">30</button>
              <button type="button" class="count-pill" data-count="45">45</button>
              <button type="button" class="count-pill" data-count="75">75</button>
            </div>
            <input type="hidden" id="count" name="count" value="15">
          </div>

          <div class="setting-group">
            <label class="toggle-card">
              <input type="checkbox" id="jee-mode" name="_jee">
              <div class="toggle-card-content">
                <div class="toggle-card-title">
                  <b>JEE Main Mode</b>
                  <span class="badge-mode">Official</span>
                </div>
                <p class="dim">Independent 60-min timer per subject, auto-switching.</p>
              </div>
            </label>
          </div>

          <div class="setting-group" id="dur-row">
            <label class="setting-lbl" for="duration">Custom Timer (Minutes)</label>
            <input id="duration" name="duration" type="number" value="30" min="5" max="180">
          </div>

          <div class="setting-group">
            <label class="toggle-card">
              <input type="checkbox" name="unseen_only" checked>
              <div class="toggle-card-content">
                <b>Unseen Questions Only</b>
                <p class="dim">Skips previously attempted questions until topics deplete.</p>
              </div>
            </label>
          </div>

          <div class="selection-summary">
            <div class="summary-stat">
              <span class="dim">Selected</span>
              <b id="pick-summary">0 Topics</b>
            </div>
            <div class="summary-stat">
              <span class="dim">Question Pool</span>
              <b id="pool-count">0 available</b>
            </div>
          </div>

          {e}

          <button class="btn-create-paper" id="submit-btn" type="submit" disabled>
            <span>Start Practice Paper</span>
            <span class="btn-subtext" id="btn-subtext">Pick topics to start</span>
          </button>
        </div>
      </div>
    </div>
  </div>
</form>

<script>
(() => {{
  const form = document.getElementById('build-form');
  const chapField = document.getElementById('chapters');
  const stField = document.getElementById('subtopics');
  const modeField = document.getElementById('mode');
  const countField = document.getElementById('count');
  const jeeBox = document.getElementById('jee-mode');
  const durRow = document.getElementById('dur-row');
  const durInput = document.getElementById('duration');
  const pickSummary = document.getElementById('pick-summary');
  const poolCount = document.getElementById('pool-count');
  const submitBtn = document.getElementById('submit-btn');
  const btnSubtext = document.getElementById('btn-subtext');

  // 1. Subject Tabs Filter
  document.querySelectorAll('.sub-tab').forEach(tab => {{
    tab.addEventListener('click', () => {{
      document.querySelectorAll('.sub-tab').forEach(t => t.classList.remove('active'));
      tab.classList.add('active');
      const target = tab.dataset.tab;
      document.querySelectorAll('.subject-section').forEach(sec => {{
        if (target === 'all' || sec.dataset.subject === target) {{
          sec.style.display = '';
        }} else {{
          sec.style.display = 'none';
        }}
      }});
    }});
  }});

  // 2. Expand/Collapse subtopics chevron button
  document.querySelectorAll('.btn-expand-subtopics').forEach(btn => {{
    btn.addEventListener('click', (e) => {{
      e.preventDefault();
      e.stopPropagation();
      const card = btn.closest('.chapter-card');
      card.classList.toggle('expanded');
      btn.setAttribute('aria-expanded', card.classList.contains('expanded'));
    }});
  }});

  // 3. Question Count Pills
  document.querySelectorAll('.count-pill').forEach(pill => {{
    pill.addEventListener('click', () => {{
      document.querySelectorAll('.count-pill').forEach(p => p.classList.remove('active'));
      pill.classList.add('active');
      countField.value = pill.dataset.count;
      // Default duration: 2 mins per question
      if (!jeeBox.checked) {{
        durInput.value = Math.max(10, Math.min(180, Number(pill.dataset.count) * 2));
      }}
      sync();
    }});
  }});

  // 4. JEE Mode Toggle
  jeeBox.addEventListener('change', () => {{
    durRow.hidden = jeeBox.checked;
    modeField.value = jeeBox.checked ? 'jee' : '';
    sync();
  }});

  // 5. Chapter Checkbox Changes
  document.querySelectorAll('.chap').forEach(ch => {{
    ch.addEventListener('change', () => {{
      const card = ch.closest('.chapter-card');
      const chapId = ch.value;
      card.querySelectorAll('.subtopic').forEach(st => {{
        st.checked = ch.checked;
      }});
      ch.indeterminate = false;
      sync();
    }});
  }});

  // 6. Subtopic Checkbox Changes
  document.querySelectorAll('.subtopic').forEach(st => {{
    st.addEventListener('change', () => {{
      const card = st.closest('.chapter-card');
      const chapBox = card.querySelector('.chap');
      const allSubs = [...card.querySelectorAll('.subtopic')];
      const anyChecked = allSubs.some(s => s.checked);
      const allChecked = allSubs.every(s => s.checked);
      chapBox.checked = anyChecked;
      chapBox.indeterminate = anyChecked && !allChecked;
      sync();
    }});
  }});

  // 7. Subtopic Tray Select/Clear Helpers
  document.querySelectorAll('.select-all-tray').forEach(btn => {{
    btn.addEventListener('click', () => {{
      const card = btn.closest('.chapter-card');
      card.querySelectorAll('.subtopic').forEach(st => st.checked = true);
      const chapBox = card.querySelector('.chap');
      chapBox.checked = true;
      chapBox.indeterminate = false;
      sync();
    }});
  }});

  document.querySelectorAll('.clear-tray').forEach(btn => {{
    btn.addEventListener('click', () => {{
      const card = btn.closest('.chapter-card');
      card.querySelectorAll('.subtopic').forEach(st => st.checked = false);
      const chapBox = card.querySelector('.chap');
      chapBox.checked = false;
      chapBox.indeterminate = false;
      sync();
    }});
  }});

  // 8. Select All Subject Button
  document.querySelectorAll('.btn-select-all-subject').forEach(btn => {{
    btn.addEventListener('click', () => {{
      const subj = btn.dataset.subj;
      const sec = btn.closest('.subject-section');
      const allChaps = [...sec.querySelectorAll('.chap')];
      const areAllSelected = allChaps.every(c => c.checked);
      const newState = !areAllSelected;
      sec.querySelectorAll('.chap, .subtopic').forEach(chk => {{
        chk.checked = newState;
        chk.indeterminate = false;
      }});
      btn.textContent = newState ? `Clear ${{subj}}` : `Select all ${{subj}}`;
      sync();
    }});
  }});

  // 9. Quick Presets
  document.querySelectorAll('.preset-btn').forEach(btn => {{
    btn.addEventListener('click', () => {{
      const preset = btn.dataset.preset;
      if (preset === 'clear') {{
        document.querySelectorAll('.chap, .subtopic').forEach(c => {{
          c.checked = false;
          c.indeterminate = false;
        }});
      }} else if (preset === 'full') {{
        document.querySelectorAll('.chap, .subtopic').forEach(c => {{
          c.checked = true;
          c.indeterminate = false;
        }});
        setCountPill('75');
        jeeBox.checked = true;
        durRow.hidden = true;
        modeField.value = 'jee';
      }} else if (preset === 'phy' || preset === 'chem' || preset === 'math') {{
        const targetSubj = preset === 'phy' ? 'Physics' : preset === 'chem' ? 'Chemistry' : 'Maths';
        document.querySelectorAll('.chap, .subtopic').forEach(c => {{
          const isTarget = c.dataset.subj === targetSubj;
          c.checked = isTarget;
          c.indeterminate = false;
        }});
        setCountPill('30');
      }}
      sync();
    }});
  }});

  function setCountPill(num) {{
    document.querySelectorAll('.count-pill').forEach(p => {{
      if (p.dataset.count === num) {{
        p.classList.add('active');
        countField.value = num;
      }} else {{
        p.classList.remove('active');
      }}
    }});
  }}

  // 10. Sync state, counter and button
  function sync() {{
    const checkedChaps = [...document.querySelectorAll('.chap:checked')];
    const checkedSubs = [...document.querySelectorAll('.subtopic:checked')];
    chapField.value = checkedChaps.map(c => c.value).join(',');
    stField.value = checkedSubs.map(s => s.value).join(',');

    let pool = 0;
    checkedChaps.forEach(c => {{
      pool += Number(c.dataset.count || 0);
    }});

    const nChapters = checkedChaps.length;
    const nSubs = checkedSubs.length;
    const qCount = countField.value || 15;

    if (nChapters === 0 && nSubs === 0) {{
      pickSummary.textContent = '0 Topics';
      poolCount.textContent = '0 available';
      submitBtn.disabled = true;
      btnSubtext.textContent = 'Select topics above to start';
    }} else {{
      pickSummary.textContent = `${{nChapters}} Units (${{nSubs}} Subtopics)`;
      poolCount.textContent = `~${{pool.toLocaleString()}} in pool`;
      submitBtn.disabled = false;
      const durationMins = jeeBox.checked ? `${{Math.min(3, Math.max(1, new Set(checkedChaps.map(c => c.dataset.subj)).size)) * 60}} Mins` : `${{durInput.value}} Mins`;
      btnSubtext.textContent = `${{qCount}} Questions · ${{durationMins}}`;
    }}
  }}

  // Initialize
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