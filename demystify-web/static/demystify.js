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
  function initClickToExplain() {
    const board = document.getElementById('board');
    if (!board) return;

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
  }

  document.addEventListener('DOMContentLoaded', () => {
    initAll();
    initLoadingState();
  });

  document.body.addEventListener('htmx:afterSwap', () => {
    initAll();
  });
})();
