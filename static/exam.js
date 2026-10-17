// Exam screen: pane navigation, palette, per-question timing and autosave.
(() => {
  const form = document.getElementById('exam-form');
  if (!form) return;

  const panes = [...document.querySelectorAll('.qpane')];
  const palBtns = [...document.querySelectorAll('#pal .pal')];
  const timerEl = document.getElementById('timer');
  const progressEl = document.getElementById('exam-progress');
  const fillEl = document.getElementById('exam-fill');
  const countEl = document.getElementById('cbt-count');
  let cur = 0;

  const entered = new Map();
  for (const p of panes) {
    const box = p.querySelector('input');
    if (box) entered.set(p.dataset.i, Number(box.dataset.t || 0));
  }  const visited = new Set();
  const saveDot = document.getElementById('save-dot');

  // Per-question stopwatch: accrues only while the pane is on screen, so
  // time spent elsewhere never counts against a question.
  let tickPid = null;
  function startTick(pane) {
    if (tickPid) clearInterval(tickPid);
    const key = pane.dataset.i;
    tickPid = setInterval(() => {
      entered.set(key, Number(entered.get(key) || 0) + 1);
    }, 1000);
  }
  document.addEventListener('visibilitychange', () => {
    if (document.hidden && tickPid) clearInterval(tickPid);
    else if (!document.hidden) startTick(panes[cur]);
  });

  function questionId(pane) {
    const radios = [...pane.querySelectorAll('input[type=radio]')];
    if (radios.length) return Number(radios[0].name.replace('q-', ''));
    const num = pane.querySelector('input[type=text]');
    return num ? Number(num.name.replace('num-', '')) : 0;
  }

  function collect(pane) {
    const radios = [...pane.querySelectorAll('input[type=radio]')];
    const picked = radios.find(r => r.checked);
    const num = pane.querySelector('input[type=text]');
    return {
      question_id: questionId(pane),
      selected: picked ? picked.value : (num ? num.value.trim() : ''),
      marked_review: pane.dataset.review === '1',
      time_secs: Number(entered.get(pane.dataset.i) || 0),
    };
  }

  window._examShow = show;
  function show(i) {
    cur = Math.max(0, Math.min(panes.length - 1, i));
    visited.add(cur);
    panes.forEach((p, k) => { p.hidden = k !== cur; });
    palBtns.forEach((b, k) => {
      b.classList.toggle('cur', k === cur);
      b.setAttribute('aria-current', k === cur ? 'true' : 'false');
    });
    const prev = panes[cur].querySelector('[data-act=prev]');
    if (prev) prev.disabled = cur === 0;

    // Only focus on a pointer device: stealing focus on a phone pops the keyboard over the options.
    if (matchMedia('(pointer: fine)').matches) {
      const f = panes[cur].querySelector('input:not([type=hidden])');
      if (f) f.focus({ preventScroll: true });
    }
    // A pane change on a phone must start the question at the top, not halfway down.
    panes[cur].scrollIntoView({ block: 'nearest' });
    history.replaceState(null, '', '#q' + (cur + 1));
    startTick(panes[cur]);
    paintProgress();
  }

  function syncPalette(pane) {
    const btn = palBtns[Number(pane.dataset.i)];
    if (!btn) return;
    const answered = collect(pane).selected !== '';
    const review = pane.dataset.review === '1';
    const seen = visited.has(Number(pane.dataset.i));
    // 5 CBT states: unseen (default), visited+blank (skip), answered, marked, answered+marked.
    btn.classList.toggle('ans', answered && !review);
    btn.classList.toggle('ansrev', answered && review);
    btn.classList.toggle('rev', !answered && review);
    btn.classList.toggle('skip', !answered && !review && seen);
    btn.classList.remove('cur');
    const mark = answered ? (review ? '✓R' : '✓') : '·';
    btn.setAttribute('aria-label', `question ${Number(pane.dataset.i) + 1}, ${answered ? 'answered' : 'blank'}${review ? ', marked for review' : ''}`);
    btn.dataset.mark = mark;
    paintProgress();
  }

  function counts() {
    let answered = 0, review = 0, ansrev = 0, visit = 0;
    for (const p of panes) {
      const c = collect(p);
      const seen = visited.has(Number(p.dataset.i));
      if (!seen) visit++;
      if (c.marked_review) { c.selected !== '' ? ansrev++ : review++; }
      else if (c.selected !== '') answered++;
    }
    return { answered, review, ansrev, visit, left: panes.length - answered - ansrev };
  }

  function paintProgress() {
    const { answered, review, ansrev, visit, left } = counts();
    const done = answered + review + ansrev;
    if (progressEl) {
      progressEl.textContent = `${done} of ${panes.length} marked · question ${cur + 1}`;
    }
    if (fillEl) fillEl.style.width = `${panes.length ? (done / panes.length) * 100 : 0}%`;
    if (countEl) {
      countEl.textContent = `${answered + ansrev} answered · ${left} left${review + ansrev ? ` · ${review + ansrev} review` : ''}`;
    }
    // Five-state readout above the palette grid, mirroring the NTA CBT legend.
    for (const li of document.querySelectorAll('#pal-stats [data-c]')) {
      const c = li.dataset.c;
      li.textContent = String(c === 'visit' ? visit : c === 'skip' ? panes.length - answered - ansrev - review - visit : c === 'ans' ? answered : c === 'rev' ? review : ansrev);
    }
    const fabCount = document.getElementById('pal-fab-count');
    if (fabCount) fabCount.textContent = left ? `· ${left} left` : '· all answered';
    const bar = document.querySelector('.exam-bar');
    if (bar) bar.classList.toggle('complete', left === 0);
  }

  let saving = false;
  let pending = false;
  async function save(all) {
    // A second change while a save is in flight is coalesced into one follow-up write.
    if (saving) { pending = all || pending; return; }
    saving = true;
    if (saveDot) saveDot.classList.add('busy');
    const panesToSave = all ? panes : [panes[cur]];
    try {
      await fetch(`${form.dataset.base}/attempt/${form.dataset.attempt}/answer`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ answers: panesToSave.map(collect) }),
      });
      if (saveDot) { saveDot.classList.remove('busy'); saveDot.classList.add('ok'); }
    } catch (_) {
      // Server unreachable: the selection stays on screen and travels with the form submit.
      if (saveDot) { saveDot.classList.remove('busy'); saveDot.classList.add('fail'); }
    }
    saving = false;
    if (saveDot && !saving) setTimeout(() => saveDot.classList.remove('ok', 'fail'), 1200);
    if (pending) { const again = pending; pending = false; save(again); }
  }

  for (const pane of panes) {
    pane.addEventListener('change', () => {
      pane.querySelectorAll('.opt').forEach(o => {
        const input = o.querySelector('input');
        o.classList.toggle('sel', !!input && input.checked);
      });
      syncPalette(pane);
      save(false);
    });
    pane.addEventListener('click', e => {
      const btn = e.target.closest('button[data-act]');
      if (!btn) return;
      const act = btn.dataset.act;
      if (act === 'prev') { show(cur - 1); }
      if (act === 'next') { show(cur + 1); }
      if (act === 'save') { save(false).then(() => { if (cur < panes.length - 1) show(cur + 1); }); }
      if (act === 'marksave') {
        pane.dataset.review = '1';
        pane.querySelector('.mflag')?.removeAttribute('hidden');
        syncPalette(pane);
        save(false).then(() => { if (cur < panes.length - 1) show(cur + 1); });
      }
      if (act === 'review') {
        pane.dataset.review = pane.dataset.review === '1' ? '0' : '1';
        btn.classList.toggle('danger', pane.dataset.review === '1');
        pane.querySelector('.mflag')?.toggleAttribute('hidden', pane.dataset.review !== '1');
        syncPalette(pane);
        save(false);
      }
      if (act === 'clear') {
        pane.querySelectorAll('input[type=radio]').forEach(r => { r.checked = false; });
        const num = pane.querySelector('input[type=text]');
        if (num) num.value = '';
        pane.querySelectorAll('.opt').forEach(o => o.classList.remove('sel'));
        syncPalette(pane);
        save(false);
      }
    });
  }

  palBtns.forEach((b, i) => b.addEventListener('click', () => show(i)));

  // On a phone the palette is a drawer, so a tap there should also close it.
  const toggle = document.getElementById('pal-toggle');
  const side = document.querySelector('.cbt-side');
  if (toggle && side) {
    const setOpen = open => {
      side.classList.toggle('open', open);
      toggle.setAttribute('aria-expanded', String(open));
      // Trap nothing, but return focus to the opener when the drawer closes.
      if (!open) toggle.focus({ preventScroll: true });
    };
    toggle.addEventListener('click', () => setOpen(!side.classList.contains('open')));
    side.addEventListener('click', e => {
      if (e.target.closest('[data-close-palette]')) { setOpen(false); return; }
      if (e.target.closest('#pal .pal') && matchMedia('(max-width: 1024px)').matches) setOpen(false);
    });
    document.addEventListener('keydown', e => { if (e.key === 'Escape') setOpen(false); });
  }

  document.addEventListener('keydown', e => {
    if (/^(INPUT|TEXTAREA|SELECT)$/.test(e.target.tagName)) return;
    if (e.key === 'ArrowRight') show(cur + 1);
    if (e.key === 'ArrowLeft') show(cur - 1);
  });

  let left = Number(timerEl.dataset.secs || 0);
  if (left > 0) {
    const tick = setInterval(() => {
      left -= 1;
      if (left <= 0) {
        clearInterval(tick);
        form.requestSubmit();
        return;
      }
      const h = Math.floor(left / 3600);
      const m = Math.floor(left / 60) % 60;
      const s = left % 60;
      const pad = n => String(n).padStart(2, '0');
      timerEl.textContent = h > 0 ? `${h}:${pad(m)}:${pad(s)}` : m > 0 ? `${m}:${pad(s)}` : `${s}s`;
      // Warn in the last ten minutes, and again in the last minute.
      timerEl.classList.toggle('urgent', left <= 600 && left > 60);
      timerEl.classList.toggle('critical', left <= 60);
    }, 1000);
  }

  form.addEventListener('submit', e => {
    const { left: blank, review } = counts();
    const msg = blank
      ? `${blank} question${blank > 1 ? 's are' : ' is'} still blank. Submit anyway?`
      : review
        ? `${review} question${review > 1 ? 's' : ''} marked for review. Submit anyway?`
        : 'Submit this paper? You cannot change answers afterwards.';
    if (!confirm(msg)) e.preventDefault();
  });

  const hash = Number((location.hash.match(/q(\d+)/) || [])[1]);
  panes.forEach(syncPalette);
  show(hash ? hash - 1 : 0);
})();
// Per-subject timers — only active in JEE mode (duration == 60×nsubjects).
// Each strip reads its initial seconds from data-secs and counts down.
// When a strip hits zero the exam auto-advances to the next subject, not the
// whole paper; only the last strip exhausting time submits the form.
(() => {
  const strips = [...document.querySelectorAll('.subj-timer')];
  if (!strips.length) return;

  const form = document.getElementById('exam-form');
  const panes = [...document.querySelectorAll('.qpane')];
  let currentStrip = 0;

  // Helper: first pane index that belongs to a given subject name.
  function firstPaneForSubj(name) {
    return panes.findIndex(p => p.querySelector('.pane-head .dim')?.textContent.includes(name));
  }
  // Helper: first pane index that does NOT belong to a given subject.
  function firstPaneAfterSubj(name) {
    return panes.findIndex(p => !p.querySelector('.pane-head .dim')?.textContent.includes(name));
  }

  function paintStrip(el, left) {
    const m = Math.floor(left / 60), s = left % 60;
    el.querySelector('b').textContent = `${m}:${String(s).padStart(2,'0')}`;
    el.classList.toggle('warn', left <= 300 && left > 0);
    el.classList.toggle('active', left > 0);
    el.classList.toggle('done', left <= 0);
  }

  function runStrip(idx) {
    const el = strips[idx];
    if (!el) { form.requestSubmit(); return; }
    el.classList.add('active');
    let left = Number(el.dataset.secs);
    const tick = setInterval(() => {
      left -= 1;
      el.dataset.secs = left;
      paintStrip(el, left);
      if (left <= 0) {
        clearInterval(tick);
        el.classList.remove('active');
        el.classList.add('done');
        currentStrip = idx + 1;
        if (currentStrip < strips.length) {
          // Auto-jump to the first unanswered question of the next subject.
          const nextSubj = strips[currentStrip]?.dataset.subj;
          const pi = nextSubj ? firstPaneForSubj(nextSubj) : -1;
          if (pi >= 0 && typeof window._examShow === 'function') window._examShow(pi);
          runStrip(currentStrip);
        } else {
          form.requestSubmit();
        }
      }
    }, 1000);
  }

  // Expose the show function from the main IIFE so the subject timer can jump.
  // exam.js main IIFE sets window._examShow after calling show().
  strips.forEach((el, i) => paintStrip(el, Number(el.dataset.secs)));
  runStrip(0);
})();
