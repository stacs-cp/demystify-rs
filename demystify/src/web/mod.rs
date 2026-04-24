pub mod puzsvg;

use crate::json::{ConstraintInstance, Problem, Puzzle, Statement};

use self::puzsvg::PuzzleDraw;

#[must_use]
pub fn base_css() -> &'static str {
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/include/base.css"))
}

#[must_use]
pub fn base_javascript() -> &'static str {
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/include/base.js"))
}

#[must_use]
pub fn create_html(puzjson: &Problem) -> String {
    let pd = PuzzleDraw::new_with_decs(&puzjson.puzzle.kind, &puzjson.puzzle.decorations);
    let svg = pd.draw_puzzle(puzjson);

    let statements = if let Some(ref state) = puzjson.state {
        let constraints = if let Some(ref statements) = state.statements {
            map_statements(statements)
        } else {
            String::new()
        };

        let description = state.description.clone().unwrap_or(String::new());

        description + "\n" + &constraints
    } else {
        String::new()
    };

    let two_div_template = r#"
    <div style="display: flex; height: 550px;">
    <div style="width: 550px; border: 1px solid black;">
        {{ svg }}
    </div>
    <div style="flex: 1; border: 1px solid black; overflow-y: auto;">
        {{ statements }}
    </div>
</div>
"#;

    let mut context = tera::Context::new();

    context.insert("statements", &statements);
    context.insert("svg", &svg.to_string());

    let main =
        tera::Tera::one_off(two_div_template, &context, false).expect("IE: Failed templating");
    let overview = render_overview(&puzjson.puzzle);
    main + &overview
}

/// Renders a collapsible overview panel showing `$#INFO` text and all constraint instances
/// grouped by their `$#CON` class, with hover-highlighting of the cells they cover.
pub fn render_overview(puzzle: &Puzzle) -> String {
    let has_info = puzzle.info.as_ref().is_some_and(|i| !i.is_empty());
    let has_classes = puzzle
        .constraint_classes
        .as_ref()
        .is_some_and(|c| !c.is_empty());

    if !has_info && !has_classes {
        return String::new();
    }

    let mut html = String::from("<div class='constraint-overview mt-2'>");

    if let Some(info) = &puzzle.info {
        for line in info {
            html.push_str("<p class='puzzle-info-line'>");
            html.push_str(line);
            html.push_str("</p>");
        }
    }

    if let Some(classes) = &puzzle.constraint_classes {
        html.push_str(
            "<details class='constraint-classes mt-1'>\
             <summary style='cursor:pointer;'>Constraint types</summary>\
             <div>",
        );
        for (class_name, instances) in classes {
            html.push_str(&render_constraint_class(class_name, instances));
        }
        html.push_str("</div></details>");
    }

    html.push_str("</div>");
    html
}

fn render_constraint_class(class_name: &str, instances: &[ConstraintInstance]) -> String {
    let mut html = format!(
        "<details class='constraint-class'>\
         <summary style='cursor:pointer; font-family:monospace;'>{} \
         <span style='color:#666;font-size:0.85em;'>({} instances)</span></summary>\
         <div style='max-height:200px; overflow-y:auto; font-size:0.85em;'>",
        class_name,
        instances.len()
    );
    for inst in instances {
        let cells_str: String = inst
            .cells
            .iter()
            .map(|[r, c]| format!("C_{}_{}", r + 1, c + 1))
            .collect::<Vec<_>>()
            .join(" ");
        html.push_str(&format!(
            "<div class='constraint-instance js_con_preview' data-cells='{}' \
             style='padding:2px 4px; cursor:default;'>{}</div>",
            cells_str, inst.description
        ));
    }
    html.push_str("</div></details>");
    html
}

fn map_statements(statements: &Vec<Statement>) -> String {
    let constraint_template = r#"
    <div class="constraintlist">
{% for statement in statements %}
    <div class="{% for class in statement.classes %}{{ class }} {% endfor %}">
        {{ statement.content }}
    </div>
{% endfor %}
</div>
"#;

    let mut context = tera::Context::new();

    context.insert("statements", statements);

    tera::Tera::one_off(constraint_template, &context, false)
        .expect("IE: Fatal internal formatting error")
}
