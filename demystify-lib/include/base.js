function applyHighlight(element) {
    const classes = element.classList;
    for (let i = 0; i < classes.length; i++) {
        const className = classes[i];
        if (className.startsWith('highlight_')) {
            const highlightedElements = document.getElementsByClassName(className);
            for (let j = 0; j < highlightedElements.length; j++) {
                highlightedElements[j].style["background-color"] = 'yellow';
                highlightedElements[j].style.fill = 'yellow';
            }
        }
    }
}

function removeHighlight(element) {
    const classes = element.classList;
    for (let i = 0; i < classes.length; i++) {
        const className = classes[i];
        if (className.startsWith('highlight_')) {
            const highlightedElements = document.getElementsByClassName(className);
            for (let j = 0; j < highlightedElements.length; j++) {
                highlightedElements[j].style.removeProperty("background-color");
                highlightedElements[j].style.removeProperty("fill");
            }
        }
    }
}

function applyHighlightFunctions() {
    const elements = document.getElementsByClassName('js_highlighter');
    for (let i = 0; i < elements.length; i++) {
        const element = elements[i];
        element.addEventListener('mouseover', () => {
            applyHighlight(element);
        });
        element.addEventListener('mouseleave', () => {
            removeHighlight(element);
        });
    }
}

function doJavascript() {
    applyHighlightFunctions();
}