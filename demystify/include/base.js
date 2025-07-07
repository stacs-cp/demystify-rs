function applyHighlight(element) {
  const classes = element.classList;
  for (let i = 0; i < classes.length; i++) {
    const className = classes[i];
    if (className.startsWith("highlight_")) {
      const highlightedElements = document.getElementsByClassName(className);
      for (let j = 0; j < highlightedElements.length; j++) {
        highlightedElements[j].classList.add("selected");
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

function doJavascript() {
  applyHighlightFunctions();

  document.addEventListener("htmx:beforeRequest", function () {
    document.querySelectorAll("button").forEach((btn) => {
      if (!btn.disabled) {
        btn.disabled = true;
        btn.dataset.htmxDisabled = "true"; // Mark this button
      }
    });
  });

  document.addEventListener("htmx:afterRequest", function () {
    document.querySelectorAll('[data-htmx-disabled="true"]').forEach((btn) => {
      btn.disabled = false;
      delete btn.dataset.htmxDisabled; // Clean up
    });
  });
}
