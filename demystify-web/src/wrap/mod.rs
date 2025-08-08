use anyhow::Context;
use axum::{Json, extract::Multipart};
use axum_session::{Session, SessionNullPool};
use once_cell::sync::Lazy;
use serde::Deserialize;
use serde_json::Value;

use std::{fs::File, io::Write, path::PathBuf, sync::Arc};

use anyhow::anyhow;

use crate::util::{self, get_solver_global, set_solver_global};

use demystify::problem::{self, planner::PuzzlePlanner, solver::PuzzleSolver};

macro_rules! include_model_file {
    ($path:expr) => {
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/", $path))
    };
}

static EXAMPLES: Lazy<[(&str, &str, &str); 4]> = Lazy::new(|| {
    [
        (
            "Sudoku",
            include_model_file!("examples/eprime/sudoku.eprime"),
            include_model_file!("examples/eprime/sudoku/puzzlingexample.param"),
        ),
        (
            "MiracleSudoku",
            include_model_file!("examples/eprime/miracle.eprime"),
            include_model_file!("examples/eprime/miracle/original.param"),
        ),
        (
            "StarBattle",
            include_model_file!("examples/eprime/star-battle.eprime"),
            include_model_file!("examples/eprime/star-battle/FATAtalkexample.param"),
        ),
        (
            "Binairo",
            include_model_file!("examples/eprime/binairo.essence"),
            include_model_file!("examples/eprime/binairo/diiscu.param"),
        ),
    ]
});

pub async fn dump_full_solve(
    session: Session<SessionNullPool>,
) -> Result<Json<Value>, util::AppError> {
    let solver = get_solver_global(&session)?;

    let mut solver = solver.lock().unwrap();

    let solve = solver.quick_solve();

    Ok(Json(serde_json::value::to_value(solve).unwrap()))
}

pub async fn best_next_step(session: Session<SessionNullPool>) -> Result<String, util::AppError> {
    let solver = get_solver_global(&session)?;

    let mut solver = solver.lock().unwrap();

    let (solve, lits) = solver.quick_solve_html_step();

    solver.mark_lits_as_deduced(&lits);

    if solve.is_empty() {
        Ok("Please upload a puzzle or select an example to begin.".to_string())
    } else {
        Ok(solve)
    }
}

pub async fn get_difficulties(session: Session<SessionNullPool>) -> Result<String, util::AppError> {
    let solver = get_solver_global(&session)?;

    let mut solver = solver.lock().unwrap();

    let solve = solver.quick_generate_html_difficulties();

    Ok(solve)
}

pub async fn refresh(session: Session<SessionNullPool>) -> Result<String, util::AppError> {
    let solver = get_solver_global(&session)?;

    let mut solver = solver.lock().unwrap();

    let (solve, _) = solver.quick_display_html_step(None);

    Ok(solve)
}

pub async fn click_literal(
    headers: axum::http::header::HeaderMap,
    session: Session<SessionNullPool>,
) -> Result<String, util::AppError> {
    let solver = get_solver_global(&session)?;

    let mut solver = solver.lock().unwrap();

    let cell = headers
        .get("hx-trigger")
        .context("Missing header: 'hx-trigger'")?;
    let cell = cell.to_str()?;
    let cell: Result<Vec<_>, _> = cell.split('_').skip(1).map(str::parse).collect();
    let cell = cell?;

    session.set("click_cell", &cell);

    let (html, lits) = solver.quick_solve_html_step_for_literal(cell);

    let lidx_lits: Vec<_> = lits.iter().map(|x| x.lidx()).collect();
    session.set("lidx_lits", &lidx_lits);

    Ok(html)
}

pub async fn upload_files(
    session: Session<SessionNullPool>,
    mut multipart: Multipart,
) -> Result<String, util::AppError> {
    let temp_dir = tempfile::tempdir().context("Failed to create temporary directory")?;

    let mut model: Option<PathBuf> = None;
    let mut param: Option<PathBuf> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .context("Failed to parse multipart upload")?
    {
        if field.name().unwrap() != "model" && field.name().unwrap() != "parameter" {
            return Err(anyhow!(
                "Form malformed -- should contain 'model' and 'parameter', but it contains '{}'",
                field.name().unwrap()
            )
            .into());
        }

        // Grab the name
        let form_file_name = field.file_name().context("No filename")?;

        eprintln!("Got file '{form_file_name}'!");

        let file_name = if form_file_name.ends_with(".param") || form_file_name.ends_with(".json") {
            if param.is_some() {
                return Err(anyhow!("Cannot upload two param files (.param or .json)").into());
            }

            if form_file_name.ends_with(".param") {
                param = Some("upload.param".into());
                "upload.param"
            } else {
                param = Some("upload.json".into());
                "upload.json"
            }
        } else if form_file_name.ends_with(".eprime") || form_file_name.ends_with(".essence") {
            if model.is_some() {
                return Err(anyhow!("Can only upload one .eprime or .essence file").into());
            }
            if form_file_name.ends_with(".eprime") {
                model = Some("upload.eprime".into());
                "upload.eprime"
            } else {
                model = Some("upload.essence".into());
                "upload.essence"
            }
        } else {
            return Err(anyhow!(
                "Only expecting .param, .json, .eprime or .essence uploads, not '{}'",
                form_file_name
            )
            .into());
        };

        // Create a path for the soon-to-be file
        let file_path = temp_dir.path().join(file_name);

        // Unwrap the incoming bytes
        let data = field.bytes().await.context("Failed to read file bytes")?;

        // Open a handle to the file
        let mut file_handle = File::create(file_path).context("Failed to open file for writing")?;

        // Write the incoming data to the handle
        file_handle
            .write_all(&data)
            .context("Failed to write data!")?;
    }

    if model.is_none() {
        return Ok(r###"
            <div class="alert alert-danger">
                <h4>Upload Error</h4>
                <p>Please upload a model file (.eprime or .essence)</p>
            </div>
        "###
        .to_string());
    }

    if param.is_none() {
        return Ok(r###"
            <div class="alert alert-danger">
                <h4>Upload Error</h4>
                <p>Please upload a parameter file (.param or .json)</p>
            </div>
        "###
        .to_string());
    }

    match load_model(&session, temp_dir, model, param) {
        Ok(_) => refresh(session).await,
        Err(e) => Ok(format!(
            r###"
            <div class="alert alert-danger">
                <h4>Failed to upload puzzle</h4>
                <pre class="text-danger">{e:#}</pre>
                <p>Please check your files and try again.</p>
            </div>
            "###
        )),
    }
}

#[derive(Deserialize)]
pub struct ExampleParams {
    example_name: String,
}

#[derive(Deserialize)]
pub struct SubmitExampleParams {
    param_content: String,
    example_name: String,
}

pub async fn load_example(
    _session: Session<SessionNullPool>,
    form: axum::extract::Form<ExampleParams>,
) -> Result<String, util::AppError> {
    let example_name = form.example_name.clone();

    let param_content = EXAMPLES
        .iter()
        .find(|(name, _, _)| *name == example_name)
        .map(|(_, _, content)| *content)
        .context(format!("Example '{example_name}' not found"))?;

    Ok(format!(
        r###"
        <h5>Edit Parameters for {example_name}</h5>
        <form id="paramForm" hx-post="/submitExample" hx-target="#mainSpace">
            <input type="hidden" name="example_name" value="{example_name}">
            <textarea name="param_content" class="form-control" rows="15" style="font-family: monospace;">{param_content}</textarea>
            <button type="submit" class="btn btn-primary mt-2" hx-indicator="#indicator">
                Submit Parameters
            </button>
        </form>
    "###
    ))
}

pub async fn get_example_names() -> String {
    let options = EXAMPLES
        .iter()
        .map(|(name, _, _)| format!("<option value=\"{name}\">{name}</option>"))
        .collect::<Vec<_>>()
        .join("");

    r###"<form id="exampleForm" hx-post="/loadExample" hx-target="#exampleParams">
                        <select name="example_name" class="form-select" required>
    "###
    .to_owned()
        + &options
        + r###" 
                        </select>
                        <button type="submit" class="btn btn-primary mt-3" hx-indicator="#indicator">
                            Load Example
                        </button>
                    </form>
    "###
}

pub async fn submit_example(
    session: Session<SessionNullPool>,
    form: axum::extract::Form<SubmitExampleParams>,
) -> Result<String, util::AppError> {
    let example_name = form.example_name.clone();
    let param_content = form.param_content.clone();

    let model_content = EXAMPLES
        .iter()
        .find(|(name, _, _)| *name == example_name)
        .map(|(_, content, _)| *content)
        .context(format!("Example '{example_name}' not found"))?;

    let temp_dir = tempfile::tempdir().context("Failed to create temporary directory")?;

    let model_dest = temp_dir.path().join("upload.eprime");
    std::fs::write(&model_dest, model_content).context("Failed to write model file")?;

    let param_dest = temp_dir.path().join("upload.param");
    std::fs::write(&param_dest, param_content).context("Failed to write param file")?;

    match load_model(
        &session,
        temp_dir,
        Some("upload.eprime".into()),
        Some("upload.param".into()),
    ) {
        Ok(_) => refresh(session).await,
        Err(e) => Ok(format!(
            r###"
            <div class="alert alert-danger">
                <h4>Failed to load puzzle</h4>
                <pre class="text-danger">{e:#}</pre>
                <p>Please check your parameter file and try again.</p>
            </div>
            "###
        )),
    }
}

fn load_model(
    session: &Session<SessionNullPool>,
    temp_dir: tempfile::TempDir,
    model: Option<PathBuf>,
    param: Option<PathBuf>,
) -> anyhow::Result<()> {
    let puzzle = problem::parse::parse_essence(
        &temp_dir.path().join(model.unwrap()),
        &temp_dir.path().join(param.unwrap()),
    )?;
    let puzzle = Arc::new(puzzle);
    let puz = PuzzleSolver::new(puzzle)?;
    let plan = PuzzlePlanner::new(puz);
    set_solver_global(session, plan);
    Ok(())
}
