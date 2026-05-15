(() => {
  // ─── Deduction hover → board cell highlighting ───
  function initDeductionHover() {
    const list = document.querySelector('.he-deductions');
    if (!list) return;
    const board = document.getElementById('board');
    if (!board) return;

    const CELLS = (sel) => board.querySelectorAll(`[data-cell="${sel}"]`);

    function clear() {
      board.querySelectorAll('[data-hl]').forEach(el => el.removeAttribute('data-hl'));
    }

    function apply(step) {
      clear();
      const t = step.dataset.targetCell;
      if (t) {
        CELLS(t).forEach(el => el.setAttribute('data-hl', 'target'));
      }

      const scopeRaw = step.dataset.scope;
      if (scopeRaw) {
        const [kind, data] = scopeRaw.split(':');
        if (kind === 'row') {
          for (let c = 1; c <= 9; c++)
            CELLS(`${data},${c}`).forEach(el => el.hasAttribute('data-hl') || el.setAttribute('data-hl', 'scope'));
        } else if (kind === 'col') {
          for (let r = 1; r <= 9; r++)
            CELLS(`${r},${data}`).forEach(el => el.hasAttribute('data-hl') || el.setAttribute('data-hl', 'scope'));
        } else if (kind === 'box') {
          const [br, bc] = data.split(',').map(Number);
          for (let r = br; r < br + 3; r++)
            for (let c = bc; c < bc + 3; c++)
              CELLS(`${r},${c}`).forEach(el => el.hasAttribute('data-hl') || el.setAttribute('data-hl', 'scope'));
        }
      }

      (step.dataset.support || '').split(/\s+/).filter(Boolean).forEach(k => {
        CELLS(k).forEach(el => el.setAttribute('data-hl', el.getAttribute('data-hl') || 'support'));
      });
    }

    list.addEventListener('mouseover', (e) => {
      const step = e.target.closest('.he-step');
      if (step) apply(step);
    });
    list.addEventListener('focusin', (e) => {
      const step = e.target.closest('.he-step');
      if (step) apply(step);
    });
    list.addEventListener('mouseleave', clear);
  }

  // ─── Constraint class highlighting (highlight_con* CSS classes) ───
  function applyHighlight(element) {
    for (const className of element.classList) {
      if (className.startsWith('highlight_')) {
        for (const el of document.getElementsByClassName(className)) {
          el.classList.add('selected');
        }
      }
    }
  }

  function removeHighlight(element) {
    for (const className of element.classList) {
      if (className.startsWith('highlight_')) {
        for (const el of document.getElementsByClassName(className)) {
          el.classList.remove('selected');
        }
      }
    }
  }

  function initHighlightFunctions() {
    for (const el of document.getElementsByClassName('js_highlighter')) {
      el.addEventListener('mouseover', () => applyHighlight(el));
      el.addEventListener('mouseleave', () => removeHighlight(el));
    }
  }

  // ─── Constraint preview (inspector: hover class → highlight cells) ───
  function applyConstraintPreview(element) {
    const cells = element.dataset.cells ? element.dataset.cells.split(' ') : [];
    cells.forEach(id => {
      const el = document.getElementById(id);
      if (el) el.classList.add('con-preview');
    });
  }

  function removeConstraintPreview(element) {
    const cells = element.dataset.cells ? element.dataset.cells.split(' ') : [];
    cells.forEach(id => {
      const el = document.getElementById(id);
      if (el) el.classList.remove('con-preview');
    });
  }

  function initConstraintPreview() {
    document.querySelectorAll('.js_con_preview').forEach(el => {
      el.addEventListener('mouseover', () => applyConstraintPreview(el));
      el.addEventListener('mouseleave', () => removeConstraintPreview(el));
    });
  }

  // ─── Click-to-explain (click a candidate cell → explain that literal) ───
  // Attach directly to the board SVG element. A per-board "bound" marker keeps
  // the listener idempotent across HTMX swaps (the SVG is freshly rendered, so
  // the marker is gone and we re-bind on the new element).
  function initClickToExplain() {
    const board = document.getElementById('board');
    if (!board) return;
    if (board.dataset.solverClickBound === '1') return;
    if (!document.getElementById('solver-stage')) return; // game pages skip this
    board.dataset.solverClickBound = '1';
    board.addEventListener('click', (e) => {
      const cand = e.target.closest('[data-cand]');
      if (!cand) return;
      const id = cand.id;
      if (!id || !id.startsWith('D_')) return;
      htmx.ajax('POST', '/solver/explain', {
        target: '#solver-stage',
        swap: 'outerHTML',
        headers: { 'X-Cell-Literal': id }
      });
    });
  }

  // ─── Game mode: left/right click → place / rule out; flash on reject ───
  let whyMode = false;

  function postGameClick(id, sign) {
    htmx.ajax('POST', `/game/click?sign=${sign}`, {
      target: '#game-stage',
      swap: 'outerHTML',
      headers: { 'X-Cell-Literal': id }
    });
  }

  function postGameWhy(id) {
    htmx.ajax('POST', '/game/hint/why', {
      target: '#game-stage',
      swap: 'outerHTML',
      headers: { 'X-Cell-Literal': id }
    });
    whyMode = false;
    const btn = document.getElementById('game-why-toggle');
    if (btn) btn.classList.remove('he-btn-active');
  }

  function initGameClicks() {
    const board = document.getElementById('board');
    if (!board) return;
    if (!document.getElementById('game-stage')) return; // solver pages skip this
    if (board.dataset.gameClickBound !== '1') {
      board.dataset.gameClickBound = '1';
      board.addEventListener('click', (e) => {
        const cand = e.target.closest('[data-cand]');
        if (!cand) return;
        const id = cand.id;
        if (!id || !id.startsWith('D_')) return;
        e.preventDefault();
        if (whyMode) { postGameWhy(id); return; }
        postGameClick(id, 'pos');
      });
      board.addEventListener('contextmenu', (e) => {
        const cand = e.target.closest('[data-cand]');
        if (!cand) return;
        e.preventDefault();
        const id = cand.id;
        if (!id || !id.startsWith('D_')) return;
        if (whyMode) { postGameWhy(id); return; }
        postGameClick(id, 'neg');
      });
    }
    const whyBtn = document.getElementById('game-why-toggle');
    if (whyBtn && whyBtn.dataset.bound !== '1') {
      whyBtn.dataset.bound = '1';
      whyBtn.addEventListener('click', () => {
        whyMode = !whyMode;
        whyBtn.classList.toggle('he-btn-active', whyMode);
      });
    }
  }

  function flashRejectedClickIfAny() {
    const stage = document.getElementById('game-stage');
    if (!stage) return;
    if (stage.dataset.gameMode !== 'true') return;
    // The server returns failures as a data attribute; we don't know which
    // cell caused the reject across the swap, so just briefly mark the whole
    // board if failures went up. Track last-seen failures on the window.
    const fails = parseInt(stage.dataset.failures || '0', 10);
    if (window.__lastGameFailures === undefined) {
      window.__lastGameFailures = fails;
      return;
    }
    if (fails > window.__lastGameFailures) {
      const figure = stage.querySelector('.he-board');
      if (figure) {
        figure.classList.add('he-game-flash-wrong');
        setTimeout(() => figure.classList.remove('he-game-flash-wrong'), 350);
      }
    }
    window.__lastGameFailures = fails;
  }

  function persistGameProgressIfWon() {
    const stage = document.getElementById('game-stage');
    if (!stage) return;
    if (stage.dataset.gameMode !== 'true') return;
    if (stage.dataset.won !== 'true') return;
    const levelId = stage.dataset.levelId;
    if (!levelId) return;
    try {
      const prog = JSON.parse(localStorage.getItem('demystify_progress') || '{}');
      const existing = prog[levelId];
      const entry = {
        failures: parseInt(stage.dataset.failures || '0', 10),
        hints_used: parseInt(stage.dataset.hints || '0', 10),
        completed_at: Date.now(),
      };
      // Keep the best run (fewest failures, then fewest hints).
      if (!existing
        || entry.failures < existing.failures
        || (entry.failures === existing.failures && entry.hints_used < existing.hints_used)) {
        prog[levelId] = entry;
        localStorage.setItem('demystify_progress', JSON.stringify(prog));
      }
    } catch (e) {
      console.warn('Could not persist progress:', e);
    }
  }

  function decorateLevelSelect() {
    const grid = document.getElementById('game-level-grid');
    if (!grid) return;
    let prog = {};
    try { prog = JSON.parse(localStorage.getItem('demystify_progress') || '{}'); } catch (e) {}
    grid.querySelectorAll('[data-badge-for]').forEach(badge => {
      const id = badge.dataset.badgeFor;
      const entry = prog[id];
      if (!entry) return;
      badge.textContent = `✓ ${entry.failures}f / ${entry.hints_used}h`;
      badge.classList.add('he-level-card-badge--done');
    });
  }

  // ─── htmx loading state ───
  function initLoadingState() {
    document.addEventListener('htmx:beforeRequest', () => {
      document.querySelectorAll('button').forEach(btn => {
        if (!btn.disabled) {
          btn.disabled = true;
          btn.dataset.htmxDisabled = 'true';
        }
      });
      document.body.style.cursor = 'wait';
    });

    document.addEventListener('htmx:afterRequest', () => {
      document.querySelectorAll('[data-htmx-disabled="true"]').forEach(btn => {
        btn.disabled = false;
        delete btn.dataset.htmxDisabled;
      });
      document.body.style.cursor = 'default';
    });
  }

  // ─── Re-init after htmx swaps ───
  function initAll() {
    initDeductionHover();
    initHighlightFunctions();
    initConstraintPreview();
    initClickToExplain();
    initGameClicks();
    flashRejectedClickIfAny();
    persistGameProgressIfWon();
    decorateLevelSelect();
  }

  document.addEventListener('DOMContentLoaded', () => {
    initAll();
    initLoadingState();
  });

  document.body.addEventListener('htmx:afterSwap', () => {
    initAll();
  });
})();
