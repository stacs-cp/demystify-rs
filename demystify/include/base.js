function applyHighlight(element) {
  const classes = element.classList;
  for (let i = 0; i < classes.length; i++) {
    const className = classes[i];
    if (className.startsWith("highlight_")) {
      console.log(className)
      const highlightedElements = document.getElementsByClassName(className);
      for (let j = 0; j < highlightedElements.length; j++) {
        highlightedElements[j].classList.add("selected");
        // Also veil the whole cell holding this literal (board SVG only —
        // the prose copies of the constraint have no [data-cell] ancestor).
        const cell = highlightedElements[j].closest("[data-cell]");
        if (cell) cell.classList.add("sel-cell");
      }
    }
  }
}

function removeHighlight(element) {
  const classes = element.classList;
  for (let i = 0; i < classes.length; i++) {
    const className = classes[i];
    if (className.startsWith("highlight_")) {
      const highlightedElements = document.getElementsByClassName(className);
      for (let j = 0; j < highlightedElements.length; j++) {
        highlightedElements[j].classList.remove("selected");
        const cell = highlightedElements[j].closest("[data-cell]");
        if (cell) cell.classList.remove("sel-cell");
      }
    }
  }
}

function applyHighlightFunctions() {
  const elements = document.getElementsByClassName("js_highlighter");
  for (let i = 0; i < elements.length; i++) {
    const element = elements[i];
    element.addEventListener("mouseover", () => {
      applyHighlight(element);
    });
    element.addEventListener("mouseleave", () => {
      removeHighlight(element);
    });
  }
}

// Constraint-preview hover applies to every cell in the document with the
// matching id, not just the first.  In single-section pages there's only
// one match per id; in multi-section walkthroughs every section's copy of
// the cell lights up together — the section being viewed always reacts,
// the others flicker harmlessly off-screen.
function applyConstraintPreview(element) {
  const cells = element.dataset.cells ? element.dataset.cells.split(" ") : [];
  cells.forEach(id => {
    document.querySelectorAll(`[id="${id}"]`).forEach(el => el.classList.add("con-preview"));
  });
}

function removeConstraintPreview(element) {
  const cells = element.dataset.cells ? element.dataset.cells.split(" ") : [];
  cells.forEach(id => {
    document.querySelectorAll(`[id="${id}"]`).forEach(el => el.classList.remove("con-preview"));
  });
}

function applyConstraintPreviewFunctions() {
  document.querySelectorAll(".js_con_preview").forEach(el => {
    el.addEventListener("mouseover", () => applyConstraintPreview(el));
    el.addEventListener("mouseleave", () => removeConstraintPreview(el));
  });
}

function doJavascript() {
  applyHighlightFunctions();
  applyConstraintPreviewFunctions();

  document.addEventListener("htmx:beforeRequest", function () {
    document.querySelectorAll("button").forEach((btn) => {
      if (!btn.disabled) {
        btn.disabled = true;
        btn.dataset.htmxDisabled = "true"; // Mark this button
      }
    });
    document.body.style.cursor = "wait";
  });

  document.addEventListener("htmx:afterRequest", function () {
    document.querySelectorAll('[data-htmx-disabled="true"]').forEach((btn) => {
      btn.disabled = false;
      delete btn.dataset.htmxDisabled; // Clean up
    });
    document.body.style.cursor = "default";
  });
}
