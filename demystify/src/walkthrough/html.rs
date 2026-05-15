//! HTML composition for walkthrough output.
//!
//! Emits one self-contained document with embedded CSS / JS (so the file
//! can be opened directly without a server).  Each `Section` becomes an
//! `<h2>` heading plus a two-column block (board SVG + statements panel),
//! mirroring `web::create_html`'s layout.

use crate::web::{base_css, base_javascript, puzsvg::PuzzleDraw, render_overview};

use super::executor::Section;

/// Compose a single HTML page from a walkthrough's rendered sections.
#[must_use]
pub fn compose_page(title: &str, sections: &[Section]) -> String {
    let mut out = String::with_capacity(64 * 1024 + sections.len() * 16 * 1024);

    out.push_str("<!doctype html>\n<html><head><meta charset=\"utf-8\">\n");
    out.push_str(&format!("<title>{}</title>\n", html_escape(title)));
    out.push_str("<style>\n");
    out.push_str(base_css());
    out.push_str(
        r#"
.walkthrough-section { margin-top: 1.5em; padding-top: 1em; border-top: 1px solid #ccc; }
.walkthrough-section:first-of-type { border-top: none; }
.walkthrough-section h2 { font-family: serif; font-size: 1.3em; }
.walkthrough-note {
    font-style: italic; color: #884400; background: #fff7ec;
    padding: 0.4em 0.8em; border-left: 3px solid #d89660;
    margin: 0.4em 0 0.8em 0; font-family: serif;
}
.walkthrough-commentary {
    color: #1a3a5a; background: #eef4fb;
    padding: 0.4em 0.8em; border-left: 3px solid #5a8ab3;
    margin: 0.4em 0 0.8em 0; font-family: serif;
}
"#,
    );
    out.push_str("</style>\n");
    out.push_str("<script>\n");
    out.push_str(base_javascript());
    out.push_str("\nwindow.addEventListener('DOMContentLoaded', doJavascript);\n");
    out.push_str("</script>\n");
    out.push_str("</head><body>\n");

    out.push_str(&format!("<h1>{}</h1>\n", html_escape(title)));

    for (idx, sec) in sections.iter().enumerate() {
        out.push_str(&format!(
            "<section class=\"walkthrough-section\" data-section=\"{idx}\">\n"
        ));
        out.push_str(&format!("<h2>{}</h2>\n", html_escape(&sec.title)));
        if let Some(n) = &sec.note {
            out.push_str(&format!(
                "<div class=\"walkthrough-note\">{}</div>\n",
                html_escape(n)
            ));
        }
        if let Some(c) = &sec.commentary {
            out.push_str(&format!(
                "<div class=\"walkthrough-commentary\">{}</div>\n",
                html_escape(c)
            ));
        }
        out.push_str(&render_section_body(sec));
        out.push_str("</section>\n");
    }

    out.push_str("</body></html>\n");
    out
}

/// Render one section's two-column block (board SVG + statements list).
/// Pulled out so callers like mystify's build_guide can use it standalone
/// without the surrounding `<html>` scaffold.
#[must_use]
pub fn render_section_body(sec: &Section) -> String {
    let pd = PuzzleDraw::new_with_decs(&sec.problem.puzzle.kind, &sec.problem.puzzle.decorations);
    let svg = pd.draw_puzzle(&sec.problem).to_string();

    let statements = sec
        .problem
        .state
        .as_ref()
        .and_then(|s| s.statements.as_ref())
        .map(|stmts| {
            let mut buf = String::from("<div class=\"constraintlist\">\n");
            for s in stmts {
                buf.push_str(&format!(
                    "<div class=\"{}\">{}</div>\n",
                    s.classes.join(" "),
                    s.content
                ));
            }
            buf.push_str("</div>\n");
            buf
        })
        .unwrap_or_default();

    let description = sec
        .problem
        .state
        .as_ref()
        .and_then(|s| s.description.clone())
        .unwrap_or_default();

    let overview = render_overview(&sec.problem.puzzle);

    format!(
        r#"<div style="display: flex; min-height: 550px;">
  <div style="width: 550px; border: 1px solid black;">{svg}</div>
  <div style="flex: 1; border: 1px solid black; overflow-y: auto; padding: 0.5em;">
    <div class="walkthrough-description">{description}</div>
    {statements}
  </div>
</div>
{overview}
"#
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
