// Exam screen: pane navigation, palette sync, per-question timing and autosave.
(() => {
  const panes = [...document.querySelectorAll('.qpane')];
  const palBtns = [...document.querySelectorAll('#pal .pal')];
  const timerEl = document.getElementById('timer');
  let cur = 0;

  const entered = new Map();
  for (const p of panes) {
    const box = p.querySelector('input');
    if (box) entered.set(p.dataset.i, Number(box.dataset.t || 0));
  }

  function show(i) {
    cur = Math.max(0, Math.min(panes.length - 1, i));
    panes.forEach((p, k) => { p.hidden = k !== cur; });
    palBtns.forEach((b, k) => b.classList.toggle('cur', k === cur));
    const focusable = panes[cur].querySelector('input:not([type=hidden])');
    if (focusable && matchMedia('(pointer: fine)').matches) focusable.focus({ preventScroll: true });
    history.replaceState(null, '', '#q' + (cur + 1));
  }

  function questionId(pane) {
    const radios = [...pane.querySelectorAll('input[type=radio]')];
    if (radios.length) return Number(radios[0].name.replace('q-', ''));
    return Number(pane.querySelector('input[type=text]').name.replace('num-', ''));
  }

  function collect(pane) {
    const i = pane.dataset.i;
    const radios = [...pane.querySelectorAll('input[type=radio]')];
    const picked = radios.find(r => r.checked);
    const num = pane.querySelector('input[type=text]');
    return {
      question_id: questionId(pane),
      selected: picked ? picked.value : (num ? num.value.trim() : ''),
      marked_review: pane.dataset.review === '1',
      time_secs: Number(entered.get(i) || 0),
    };
  }

  function syncPalette(pane) {
    const btn = palBtns[Number(pane.dataset.i)];
    if (!btn) return;
    const answered = collect(pane).selected !== '';
    const review = pane.dataset.review === '1';
    btn.classList.toggle('ans', answered && !review);
    btn.classList.toggle('rev', answered && review);
    btn.classList.toggle('skip', !answered);
    btn.classList.remove('cur');
  }

  let saving = false;
  async function save(all) {
    if (saving) return;
    saving = true;
    const panesToSave = all ? panes : [panes[cur]];
    const attemptId = document.getElementById('exam-form').dataset.attempt;
    try {
      await fetch(`/attempt/${attemptId}/answer`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ answers: panesToSave.map(collect) }),
      });
    } catch (_) {
      // Server unreachable: the selection stays on screen and travels with the form submit.
    }
    saving = false;
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
    pane.addEventListener('click', (e) => {
      const btn = e.target.closest('button[data-act]');
      if (!btn) return;
      const act = btn.dataset.act;
      if (act === 'save') { save(false).then(() => show(cur + 1)); }
      if (act === 'review') {
        pane.dataset.review = pane.dataset.review === '1' ? '0' : '1';
        btn.classList.toggle('danger', pane.dataset.review === '1');
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

  document.addEventListener('keydown', e => {
    if (/^(INPUT|TEXTAREA|SELECT)$/.test(e.target.tagName)) return;
    if (e.key === 'ArrowRight') show(cur + 1);
    if (e.key === 'ArrowLeft') show(cur - 1);
  });

  let left = Number(timerEl.dataset.secs || 0);
  if (left > 0) {
    setInterval(() => {
      left -= 1;
      if (left <= 0) {
        clearInterval(this);
        document.getElementById('exam-form').requestSubmit();
        return;
      }
      const h = String(Math.floor(left / 3600)).padStart(2, '0');
      const m = String(Math.floor(left / 60) % 60).padStart(2, '0');
      const s = String(left % 60).padStart(2, '0');
      timerEl.textContent = `${h}:${m}:${s}`;
    }, 1000);
  }

  document.getElementById('exam-form').addEventListener('submit', e => {
    if (!confirm('Submit this paper? You cannot change answers afterwards.')) e.preventDefault();
  });

  const hash = Number((location.hash.match(/q(\d+)/) || [])[1]);
  show(hash ? hash - 1 : 0);
  if (cur === 0 && !hash) panes.forEach(syncPalette);
})();