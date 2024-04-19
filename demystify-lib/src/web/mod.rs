pub mod puzsvg;

use crate::json::{Problem, Statement};

use self::puzsvg::PuzzleDraw;

pub fn create_html(puzjson: &Problem) -> String {
    let pd = PuzzleDraw::new();
    let svg = pd.draw_puzzle(puzjson);
    let statements = if let Some(ref state) = puzjson.state {
        if let Some(ref statements) = state.statements {
            map_statements(statements)
        } else {
            "".to_string()
        }
    } else {
        "".to_string()
    };

    svg.to_string() + "\n" + &statements
}

fn map_statements(statements: &Vec<Statement>) -> String {
    let constraint_template = r#"
    <div class="constraintlist">
{% for statement in statements %}
    <div class="{% for class in statement.classes %}{{ class }} {% endfor %}">
        {{ statement.content }}
    </div>
    </div>
{% endfor %}
"#;

    let mut context = tera::Context::new();

    context.insert("statements", statements);

    tera::Tera::one_off(constraint_template, &context, true)
        .expect("IE: Fatal internal formatting error")
}
