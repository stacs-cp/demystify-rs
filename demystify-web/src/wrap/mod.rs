use anyhow::Context;
use axum::{
    extract::{Multipart, Query},
    Json,
};
use axum_session::{Session, SessionNullPool};
use serde::Deserialize;
use serde_json::Value;

use std::{fs::File, io::Write, path::PathBuf, sync::Arc};

use anyhow::anyhow;

use crate::util::{self, get_solver_global, set_solver_global};

use demystify_lib::problem::{self, planner::PuzzlePlanner, solver::PuzzleSolver};

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

    let (html, lits) = solver.quick_solve_html_step_for_literal(dbg!(cell));

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
        return Err(anyhow!("Did not upload a .eprime or .essence file").into());
    }

    if param.is_none() {
        return Err(anyhow!("Did not upload a param file (either .eprime or .json) file").into());
    }

    load_model(session, temp_dir, model, param)?;

    Ok("upload successful!".to_string())
}

#[derive(Deserialize)]
pub struct ExampleParams {
    example_name: String,
}

pub async fn load_example(
    session: Session<SessionNullPool>,
    form: axum::extract::Form<ExampleParams>,
) -> Result<String, util::AppError> {
    // Extract the example name from the headers
    let example_name = form.example_name.clone();

    // Mock example database - maps example names to model and param file paths
    // Define a macro to include file contents
    macro_rules! include_model_file {
        ($path:expr) => {
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/", $path))
        };
    }

    // Store examples with their embedded content
    let examples = [
        (
            "Sudoku",
            (
                include_model_file!("../eprime/sudoku.eprime"),
                include_model_file!("../eprime/sudoku/puzzlingexample.param"),
            ),
        ),
        (
            "MiracleSudoku",
            (
                include_model_file!("../eprime/miracle.eprime"),
                include_model_file!("../eprime/miracle/original.param"),
            ),
        ),
        (
            "StarBattle",
            (
                include_model_file!("../eprime/star-battle.eprime"),
                include_model_file!("../eprime/star-battle/FATAtalkexample.param"),
            ),
        ),
    ];

    // Look up the selected example
    let (model_content, param_content) = examples
        .iter()
        .find(|(name, _)| *name == example_name)
        .map(|(_, content)| content)
        .context(format!("Example '{}' not found", example_name))?;

    // Create temporary directory
    let temp_dir = tempfile::tempdir().context("Failed to create temporary directory")?;

    // Write the model file to the temporary directory
    let model_dest = temp_dir.path().join("upload.eprime");
    std::fs::write(&model_dest, model_content).context("Failed to write model file")?;

    // Write the param file to the temporary directory
    let param_dest = temp_dir.path().join("upload.param");
    std::fs::write(&param_dest, param_content).context("Failed to write param file")?;

    // Load the model
    load_model(
        session,
        temp_dir,
        Some("upload.eprime".into()),
        Some("upload.param".into()),
    )?;

    Ok("Example loaded successfully!".to_string())
}

fn load_model(
    session: Session<SessionNullPool>,
    temp_dir: tempfile::TempDir,
    model: Option<PathBuf>,
    param: Option<PathBuf>,
) -> Result<(), util::AppError> {
    let puzzle = problem::parse::parse_essence(
        &temp_dir.path().join(model.unwrap()),
        &temp_dir.path().join(param.unwrap()),
    )?;
    let puzzle = Arc::new(puzzle);
    let puz = PuzzleSolver::new(puzzle)?;
    let plan = PuzzlePlanner::new(puz);
    set_solver_global(&session, plan);
    Ok(())
}
