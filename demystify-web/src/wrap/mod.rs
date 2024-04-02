use anyhow::Context;
use axum::{extract::Multipart, Json};
use axum_session::{Session, SessionNullPool};
use serde_json::Value;

use std::{fs::File, io::Write, path::PathBuf};

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
        if field.name().unwrap() != "files[]" {
            return Err(anyhow!(
                "Form malformed -- should contain 'files[]', but it contains '{}'",
                field.name().unwrap()
            )
            .into());
        }

        // Grab the name
        let form_file_name = field.file_name().context("No filename")?;

        println!("Got file '{form_file_name}'!");

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

    let puzzle = problem::parse::parse_essence(
        &temp_dir.path().join(model.unwrap()),
        &temp_dir.path().join(param.unwrap()),
    )?;

    let puz = PuzzleSolver::new(puzzle)?;

    let plan = PuzzlePlanner::new(puz);

    set_solver_global(&session, plan);

    Ok("upload successful!".to_string())
}
