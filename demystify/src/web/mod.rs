pub mod puzsvg;

use crate::json::{Problem, Statement};

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
    let pd = PuzzleDraw::new(&puzjson.puzzle.kind);
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

    tera::Tera::one_off(two_div_template, &context, false).expect("IE: Failed templating")
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
