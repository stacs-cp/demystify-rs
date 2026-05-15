use anyhow::Context;
use axum::body::Body;
use axum::extract::{Multipart, State};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum_session::{Session, SessionNullPool};
use once_cell::sync::Lazy;
use serde::Deserialize;

use std::{fs::File, io::Write, path::PathBuf, sync::Arc};

use anyhow::anyhow;

use crate::util::{
    self, AppState, ExploreState, SolverSession, get_solver_global, set_solver_global,
    set_solver_global_session,
};

use demystify::json::{Problem, Statement};
use demystify::problem::{
    self,
    planner::PuzzlePlanner,
    solver::PuzzleSolver,
    solvetree::{MergeStrategy, SolveTree, SolveTreeConfig},
};
use demystify::web::puzsvg::PuzzleDraw;

macro_rules! include_model_file {
    ($path:expr) => {
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/", $path))
    };
}

pub struct ExampleInfo {
    pub name: &'static str,
    pub description: &'static str,
    model: &'static str,
    param: &'static str,
}

static EXAMPLES: Lazy<[ExampleInfo; 14]> = Lazy::new(|| {
    [
        ExampleInfo {
            name: "Sudoku",
            description: "Place 1-9 in each row, column and 3x3 box exactly once.",
            model: include_model_file!("examples/eprime/sudoku.eprime"),
            param: include_model_file!("examples/eprime/sudoku/puzzlingexample.param"),
        },
        ExampleInfo {
            name: "MiracleSudoku",
            description: "Sudoku with extra constraints: no two equal digits may be a king or knight's move apart.",
            model: include_model_file!("examples/eprime/miracle.eprime"),
            param: include_model_file!("examples/eprime/miracle/original.param"),
        },
        ExampleInfo {
            name: "StarBattle",
            description: "Place stars in the grid so each row, column and region has exactly one star; stars may not touch.",
            model: include_model_file!("examples/eprime/star-battle.eprime"),
            param: include_model_file!("examples/eprime/star-battle/FATAtalkexample.param"),
        },
        ExampleInfo {
            name: "Binairo",
            description: "Fill the grid with 0s and 1s: equal counts per row/column, no three equal values adjacent.",
            model: include_model_file!("examples/eprime/binairo.essence"),
            param: include_model_file!("examples/eprime/binairo/diiscu.param"),
        },
        ExampleInfo {
            name: "Thermometer",
            description: "Fill thermometers so values increase from bulb to tip.",
            model: include_model_file!("examples/eprime/thermometer.eprime"),
            param: include_model_file!("examples/eprime/thermometer/thermometer-1.param"),
        },
        ExampleInfo {
            name: "Futoshiki",
            description: "Place digits 1-N in each row and column; respect the inequality signs between cells.",
            model: include_model_file!("examples/eprime/futoshiki.eprime"),
            param: include_model_file!("examples/eprime/futoshiki/nfutoshiki-1.param"),
        },
        ExampleInfo {
            name: "KillerSudoku",
            description: "Sudoku where caged groups of cells must sum to a given total; no repeats within a cage.",
            model: include_model_file!("examples/eprime/killersudoku.eprime"),
            param: include_model_file!("examples/eprime/killersudoku/killersudoku.param"),
        },
        ExampleInfo {
            name: "Skyscrapers",
            description: "Place 1-N in each row and column; clues on edges indicate how many buildings are visible.",
            model: include_model_file!("examples/eprime/skyscrapers.eprime"),
            param: include_model_file!("examples/eprime/skyscrapers/skyscrapers-1.param"),
        },
        ExampleInfo {
            name: "XSums",
            description: "Sudoku variant where edge clues equal the sum of the first X digits in that row/column.",
            model: include_model_file!("examples/eprime/x-sums.eprime"),
            param: include_model_file!("examples/eprime/x-sums/easy-xsums.param"),
        },
        ExampleInfo {
            name: "Kakurasu",
            description: "Place marks in a grid so each row and column's marked indices sum to the clue.",
            model: include_model_file!("examples/eprime/kakurasu.eprime"),
            param: include_model_file!("examples/eprime/kakurasu/kakurasu.param"),
        },
        ExampleInfo {
            name: "Akari",
            description: "Place light bulbs so every cell is lit; bulbs may not illuminate each other.",
            model: include_model_file!("examples/eprime/akari.eprime"),
            param: include_model_file!("examples/eprime/akari/akari-5x5.param"),
        },
        ExampleInfo {
            name: "Mosaic",
            description: "Fill each cell black or white; a numbered cell indicates how many of its neighbourhood are black.",
            model: include_model_file!("examples/eprime/mosaic.eprime"),
            param: include_model_file!("examples/eprime/mosaic/mosaic-5x5.param"),
        },
        ExampleInfo {
            name: "Nonogram",
            description: "Fill rows and columns according to clue sequences that describe runs of consecutive filled cells.",
            model: include_model_file!("examples/eprime/nonogram.eprime"),
            param: include_model_file!("examples/eprime/nonogram/duck-8x9.param"),
        },
        ExampleInfo {
            name: "Minesweeper",
            description: "Identify mine locations; numbered cells show exactly how many of their neighbours are mines.",
            model: include_model_file!("examples/eprime/minesweeper.eprime"),
            param: include_model_file!("examples/eprime/minesweeper/minesweeper-5x5.param"),
        },
    ]
});

// ─── Template rendering helpers ───

fn render_svg(problem: &Problem) -> String {
    let pd = PuzzleDraw::new_with_decs(&problem.puzzle.kind, &problem.puzzle.decorations);
    pd.draw_puzzle(problem).to_string()
}

fn count_givens(problem: &Problem) -> u32 {
    problem.puzzle.start_grid.as_ref().map_or(0, |sg| {
        sg.iter()
            .flat_map(|row| row.iter())
            .filter(|c| c.is_some())
            .count() as u32
    })
}

fn count_placed(problem: &Problem) -> u32 {
    problem
        .state
        .as_ref()
        .and_then(|s| s.knowledge_grid.as_ref())
        .map_or(0, |kg| {
            kg.iter()
                .flat_map(|row| row.iter())
                .filter(|cell| {
                    cell.as_ref().is_some_and(|vals| {
                        vals.len() == 1
                            && vals[0]
                                .classes
                                .as_ref()
                                .is_some_and(|c| c.contains("litknown") || c.contains("litpos"))
                    })
                })
                .count() as u32
        })
}

fn count_candidates(problem: &Problem) -> u32 {
    problem
        .state
        .as_ref()
        .and_then(|s| s.knowledge_grid.as_ref())
        .map_or(0, |kg| {
            kg.iter()
                .flat_map(|row| row.iter())
                .filter_map(|cell| cell.as_ref())
                .map(|vals| vals.len() as u32)
                .sum()
        })
}

fn count_constraints(problem: &Problem) -> u32 {
    problem
        .puzzle
        .constraint_classes
        .as_ref()
        .map_or(0, |cc| cc.values().map(|v| v.len() as u32).sum())
}

/// Extract deduction data from statements for template rendering.
fn extract_deductions(statements: &[Statement]) -> Vec<tera::Value> {
    let mut deductions = Vec::new();
    let mut current_result: Option<String> = None;
    let mut current_constraints: Vec<String> = Vec::new();
    let mut current_constraint_classes: Vec<Vec<String>> = Vec::new();

    for stmt in statements {
        let is_result = stmt.classes.is_empty()
            || (!stmt.classes.iter().any(|c| c.starts_with("highlight_con")));
        if is_result {
            if let Some(result) = current_result.take() {
                let mut obj = serde_json::Map::new();
                obj.insert("result".into(), tera::Value::String(result));
                obj.insert(
                    "constraints".into(),
                    tera::Value::Array(
                        current_constraints
                            .drain(..)
                            .map(tera::Value::String)
                            .collect(),
                    ),
                );
                obj.insert(
                    "constraint_classes".into(),
                    tera::Value::Array(
                        current_constraint_classes
                            .drain(..)
                            .map(|cc| {
                                tera::Value::Array(
                                    cc.into_iter().map(tera::Value::String).collect(),
                                )
                            })
                            .collect(),
                    ),
                );
                obj.insert("classes".into(), tera::Value::Array(vec![]));
                deductions.push(tera::Value::Object(obj));
            }
            current_result = Some(stmt.content.clone());
        } else {
            current_constraints.push(stmt.content.clone());
            current_constraint_classes.push(stmt.classes.clone());
        }
    }

    if let Some(result) = current_result {
        let mut obj = serde_json::Map::new();
        obj.insert("result".into(), tera::Value::String(result));
        obj.insert(
            "constraints".into(),
            tera::Value::Array(
                current_constraints
                    .into_iter()
                    .map(tera::Value::String)
                    .collect(),
            ),
        );
        obj.insert(
            "constraint_classes".into(),
            tera::Value::Array(
                current_constraint_classes
                    .into_iter()
                    .map(|cc| tera::Value::Array(cc.into_iter().map(tera::Value::String).collect()))
                    .collect(),
            ),
        );
        obj.insert("classes".into(), tera::Value::Array(vec![]));
        deductions.push(tera::Value::Object(obj));
    }

    deductions
}

fn build_solver_stage_context(
    problem: &Problem,
    round: u32,
    session: &SolverSession,
) -> tera::Context {
    let mut ctx = tera::Context::new();
    let svg = render_svg(problem);
    let puzzle_name = &problem.puzzle.kind;
    let dim = format!("{}x{}", problem.puzzle.width, problem.puzzle.height);
    let givens = count_givens(problem);
    let description = problem
        .state
        .as_ref()
        .and_then(|s| s.description.clone())
        .unwrap_or_default();

    let statements = problem.state.as_ref().and_then(|s| s.statements.as_ref());

    let all_are_legend = statements.is_some_and(|stmts| {
        !stmts.is_empty()
            && stmts
                .iter()
                .all(|s| s.classes.iter().any(|c| c.starts_with("highlight_con")))
    });

    let deductions = if all_are_legend {
        Vec::new()
    } else {
        statements.map_or_else(Vec::new, |stmts| extract_deductions(stmts))
    };

    let legend_items: Vec<tera::Value> = if all_are_legend {
        statements
            .unwrap()
            .iter()
            .map(|s| {
                let mut obj = serde_json::Map::new();
                obj.insert("content".into(), tera::Value::String(s.content.clone()));
                obj.insert(
                    "classes".into(),
                    tera::Value::Array(
                        s.classes
                            .iter()
                            .map(|c| tera::Value::String(c.clone()))
                            .collect(),
                    ),
                );
                tera::Value::Object(obj)
            })
            .collect()
    } else {
        Vec::new()
    };

    let placed = count_placed(problem);
    let total_cells = (problem.puzzle.width * problem.puzzle.height) as u32;
    let candidates = count_candidates(problem);
    let constraint_count = count_constraints(problem);
    let this_round = problem
        .state
        .as_ref()
        .and_then(|s| s.knowledge_grid.as_ref())
        .map_or(0u32, |kg| {
            kg.iter()
                .flat_map(|row| row.iter())
                .filter(|cell| {
                    cell.as_ref().is_some_and(|vals| {
                        vals.iter()
                            .any(|v| v.classes.as_ref().is_some_and(|c| c.contains("litpos")))
                    })
                })
                .count() as u32
        });

    let techniques: Vec<tera::Value> =
        problem
            .puzzle
            .constraint_classes
            .as_ref()
            .map_or_else(Vec::new, |cc| {
                cc.iter()
                    .map(|(name, instances)| {
                        let mut obj = serde_json::Map::new();
                        obj.insert("name".into(), tera::Value::String(name.clone()));
                        obj.insert(
                            "count".into(),
                            tera::Value::Number(serde_json::Number::from(instances.len())),
                        );
                        let inst_vals: Vec<tera::Value> = instances
                            .iter()
                            .map(|inst| {
                                let cell_ids = inst
                                    .cells
                                    .iter()
                                    .map(|[r, c]| format!("C_{}_{}", r + 1, c + 1))
                                    .collect::<Vec<_>>()
                                    .join(" ");
                                let mut iobj = serde_json::Map::new();
                                iobj.insert(
                                    "description".into(),
                                    tera::Value::String(inst.description.clone()),
                                );
                                iobj.insert("cell_ids".into(), tera::Value::String(cell_ids));
                                tera::Value::Object(iobj)
                            })
                            .collect();
                        obj.insert("instances".into(), tera::Value::Array(inst_vals));
                        tera::Value::Object(obj)
                    })
                    .collect()
            });

    ctx.insert("board_svg", &svg);
    ctx.insert("puzzle_name", puzzle_name);
    ctx.insert("puzzle_dim", &dim);
    ctx.insert("givens_count", &givens);
    ctx.insert("round", &round);
    ctx.insert("description", &description);
    ctx.insert("deductions", &deductions);
    ctx.insert("legend_items", &legend_items);
    ctx.insert("placed", &placed);
    ctx.insert("total_cells", &total_cells);
    ctx.insert("this_round", &this_round);
    ctx.insert("candidates_left", &candidates);
    ctx.insert("constraint_count", &constraint_count);
    ctx.insert("techniques", &techniques);

    let puzzle_info: Vec<String> = problem.puzzle.info.as_ref().cloned().unwrap_or_default();
    ctx.insert("puzzle_info", &puzzle_info);

    ctx.insert("history_len", &session.history.len());

    ctx.insert("explore_mode", &session.explore_enabled);
    if let Some(ref explore) = session.explore {
        ctx.insert("explore_active", &true);
        ctx.insert("explore_index", &(explore.current_index + 1));
        ctx.insert("explore_total", &explore.all_muses.len());
        ctx.insert(
            "explore_mus_size",
            &explore.all_muses[explore.current_index].mus_len(),
        );
    } else {
        ctx.insert("explore_active", &false);
        ctx.insert("explore_index", &0u32);
        ctx.insert("explore_total", &0u32);
        ctx.insert("explore_mus_size", &0u32);
    }

    ctx
}

fn build_solver_page_context(
    problem: &Problem,
    round: u32,
    session: &SolverSession,
) -> tera::Context {
    let mut ctx = build_solver_stage_context(problem, round, session);
    ctx.insert("view", "solver");
    ctx.insert("headline", &problem.puzzle.kind);
    ctx
}

// ─── Route handlers ───

pub async fn landing(State(state): State<AppState>) -> Result<Html<String>, util::AppError> {
    let mut ctx = tera::Context::new();
    ctx.insert("view", "landing");
    ctx.insert("puzzle_count", &EXAMPLES.len());
    ctx.insert("model_loaded", &false);

    let examples: Vec<tera::Value> = EXAMPLES
        .iter()
        .map(|ex| {
            let mut obj = serde_json::Map::new();
            obj.insert("name".into(), tera::Value::String(ex.name.to_string()));
            obj.insert(
                "description".into(),
                tera::Value::String(ex.description.to_string()),
            );
            tera::Value::Object(obj)
        })
        .collect();
    ctx.insert("examples", &examples);

    let html = state.tera.render("landing.html", &ctx)?;
    Ok(Html(html))
}

pub async fn solver_page(
    State(state): State<AppState>,
    session: Session<SessionNullPool>,
) -> Result<Response, util::AppError> {
    let solver = match get_solver_global(&session) {
        Ok(s) => s,
        Err(_) => {
            let mut ctx = tera::Context::new();
            ctx.insert("view", "solver");
            let html = state.tera.render("no_puzzle.html", &ctx)?;
            return Ok(Html(html).into_response());
        }
    };

    let mut solver = solver.lock().unwrap();
    let round: u32 = session.get("round").unwrap_or(0);
    let (problem, _lits) = solver.planner.refresh_problem();

    let ctx = build_solver_page_context(&problem, round, &solver);
    let html = state.tera.render("solver.html", &ctx)?;
    Ok(Html(html).into_response())
}

pub async fn solver_advance(
    State(state): State<AppState>,
    session: Session<SessionNullPool>,
) -> Result<Html<String>, util::AppError> {
    let solver = get_solver_global(&session)?;
    let mut solver = solver.lock().unwrap();

    if solver.explore_enabled {
        return Err(anyhow!("Disable explore mode before advancing").into());
    }

    let mut round: u32 = session.get("round").unwrap_or(0);
    round += 1;
    session.set("round", round);

    solver.snapshot();
    let (problem, _lits) = solver.planner.solve_step();
    let ctx = build_solver_stage_context(&problem, round, &solver);
    let html = state.tera.render("partials/solver_stage.html", &ctx)?;
    Ok(Html(html))
}

pub async fn solver_reset(
    State(state): State<AppState>,
    session: Session<SessionNullPool>,
) -> Result<Html<String>, util::AppError> {
    let solver = get_solver_global(&session)?;
    let mut solver = solver.lock().unwrap();
    let round: u32 = session.get("round").unwrap_or(0);

    let (problem, _lits) = solver.planner.refresh_problem();
    let ctx = build_solver_stage_context(&problem, round, &solver);
    let html = state.tera.render("partials/solver_stage.html", &ctx)?;
    Ok(Html(html))
}

#[derive(Deserialize)]
pub struct StepParam {
    step: usize,
}

pub async fn solver_goto(
    State(state): State<AppState>,
    session: Session<SessionNullPool>,
    form: axum::extract::Form<StepParam>,
) -> Result<Html<String>, util::AppError> {
    let solver = get_solver_global(&session)?;
    let mut solver = solver.lock().unwrap();

    solver.goto_step(form.step)?;
    session.set("round", form.step as u32);

    let (problem, _lits) = solver.planner.refresh_problem();
    let ctx = build_solver_stage_context(&problem, form.step as u32, &solver);
    let html = state.tera.render("partials/solver_stage.html", &ctx)?;
    Ok(Html(html))
}

pub async fn solver_difficulties(
    State(state): State<AppState>,
    session: Session<SessionNullPool>,
) -> Result<Html<String>, util::AppError> {
    let solver = get_solver_global(&session)?;
    let mut solver = solver.lock().unwrap();
    let round: u32 = session.get("round").unwrap_or(0);

    let problem = solver.planner.difficulty_problem(false);
    let ctx = build_solver_stage_context(&problem, round, &solver);
    let html = state.tera.render("partials/solver_stage.html", &ctx)?;
    Ok(Html(html))
}

pub async fn solver_explain(
    headers: axum::http::header::HeaderMap,
    State(state): State<AppState>,
    session: Session<SessionNullPool>,
) -> Result<Html<String>, util::AppError> {
    let solver = get_solver_global(&session)?;
    let mut solver = solver.lock().unwrap();

    let cell = parse_cell_literal(&headers)?;

    if solver.explore_enabled {
        let round: u32 = session.get("round").unwrap_or(0);
        return render_explore_explain(&state, &mut solver, round, cell);
    }

    solver.snapshot();
    let mut round: u32 = session.get("round").unwrap_or(0);
    round += 1;
    session.set("round", round);

    let (problem, _lits) = solver.planner.solve_step_for_literal(cell);
    let ctx = build_solver_stage_context(&problem, round, &solver);
    let html = state.tera.render("partials/solver_stage.html", &ctx)?;
    Ok(Html(html))
}

// ─── Explore mode ───

fn parse_cell_literal(headers: &axum::http::header::HeaderMap) -> anyhow::Result<Vec<i64>> {
    let cell = headers
        .get("x-cell-literal")
        .context("Missing header: 'X-Cell-Literal'")?;
    let cell = cell.to_str()?;
    let cell: Result<Vec<_>, _> = cell.split('_').skip(1).map(str::parse).collect();
    Ok(cell?)
}

pub fn parse_cell_literal_pub(headers: &axum::http::header::HeaderMap) -> anyhow::Result<Vec<i64>> {
    parse_cell_literal(headers)
}

pub fn build_solver_stage_context_pub(
    problem: &Problem,
    round: u32,
    session: &SolverSession,
) -> tera::Context {
    build_solver_stage_context(problem, round, session)
}

fn render_explore_explain(
    state: &AppState,
    solver: &mut SolverSession,
    round: u32,
    cell: Vec<i64>,
) -> Result<Html<String>, util::AppError> {
    let all_muses = solver.planner.all_muses_for_literal(cell);
    if all_muses.is_empty() {
        let (problem, _) = solver.planner.refresh_problem();
        let ctx = build_solver_stage_context(&problem, round, solver);
        return Ok(Html(state.tera.render("partials/solver_stage.html", &ctx)?));
    }
    let problem = solver.planner.preview_mus(&all_muses[0]);
    solver.explore = Some(ExploreState {
        all_muses,
        current_index: 0,
    });
    let ctx = build_solver_stage_context(&problem, round, solver);
    Ok(Html(state.tera.render("partials/solver_stage.html", &ctx)?))
}

pub async fn solver_explore_toggle(
    State(state): State<AppState>,
    session: Session<SessionNullPool>,
) -> Result<Html<String>, util::AppError> {
    let solver = get_solver_global(&session)?;
    let mut solver = solver.lock().unwrap();
    let round: u32 = session.get("round").unwrap_or(0);

    solver.explore_enabled = !solver.explore_enabled;
    if !solver.explore_enabled {
        solver.explore = None;
    }

    let (problem, _lits) = solver.planner.refresh_problem();
    let ctx = build_solver_stage_context(&problem, round, &solver);
    let html = state.tera.render("partials/solver_stage.html", &ctx)?;
    Ok(Html(html))
}

pub async fn solver_explore_navigate(
    headers: axum::http::header::HeaderMap,
    State(state): State<AppState>,
    session: Session<SessionNullPool>,
) -> Result<Html<String>, util::AppError> {
    let solver = get_solver_global(&session)?;
    let mut solver = solver.lock().unwrap();
    let round: u32 = session.get("round").unwrap_or(0);

    let direction = headers
        .get("x-explore-direction")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("next");

    let explore = solver
        .explore
        .as_mut()
        .context("No explore state — click a cell first")?;

    match direction {
        "prev" => {
            if explore.current_index > 0 {
                explore.current_index -= 1;
            }
        }
        _ => {
            if explore.current_index + 1 < explore.all_muses.len() {
                explore.current_index += 1;
            }
        }
    }

    let idx = explore.current_index;
    let mus = solver.explore.as_ref().unwrap().all_muses[idx].clone();
    let problem = solver.planner.preview_mus(&mus);

    let ctx = build_solver_stage_context(&problem, round, &solver);
    let html = state.tera.render("partials/solver_stage.html", &ctx)?;
    Ok(Html(html))
}

// ─── Upload / example loading ───

#[derive(Deserialize)]
pub struct ExampleParams {
    example_name: String,
}

#[derive(Deserialize)]
pub struct SubmitExampleParams {
    example_name: String,
    param_content: String,
}

pub async fn upload_files(
    State(state): State<AppState>,
    session: Session<SessionNullPool>,
    mut multipart: Multipart,
) -> Result<Response, util::AppError> {
    let temp_dir = tempfile::Builder::new()
        .prefix(".demystify-")
        .tempdir_in(".")
        .context("Failed to create temporary directory")?;

    let mut model: Option<PathBuf> = None;
    let mut param: Option<PathBuf> = None;
    let mut assignment: Option<PathBuf> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .context("Failed to parse multipart upload")?
    {
        let field_name = field.name().unwrap_or("").to_owned();
        if field_name != "model" && field_name != "params" && field_name != "assignment" {
            continue;
        }
        let form_file_name = field.file_name().context("No filename")?.to_owned();

        // Multipart fields with no chosen file send an empty filename — let
        // the user skip the optional `assignment` slot without it counting.
        if form_file_name.is_empty() {
            continue;
        }

        let file_name = if field_name == "assignment" {
            if !form_file_name.ends_with(".json") {
                return Err(anyhow!(
                    "Puzzle assignment must be a .json file, got '{form_file_name}'"
                )
                .into());
            }
            if assignment.is_some() {
                return Err(anyhow!("Cannot upload two assignment files").into());
            }
            assignment = Some("upload.assignment.json".into());
            "upload.assignment.json"
        } else if form_file_name.ends_with(".param") || form_file_name.ends_with(".json") {
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
                "Only expecting .param, .json, .eprime or .essence uploads, not '{form_file_name}'"
            )
            .into());
        };

        let file_path = temp_dir.path().join(file_name);
        let data = field.bytes().await.context("Failed to read file bytes")?;
        let mut file_handle = File::create(file_path).context("Failed to open file for writing")?;
        file_handle
            .write_all(&data)
            .context("Failed to write data")?;
    }

    if model.is_none() {
        return Err(anyhow!("Please upload a model file (.eprime or .essence)").into());
    }
    if param.is_none() {
        return Err(anyhow!("Please upload a parameter file (.param or .json)").into());
    }

    load_model(&session, &state, temp_dir, model, param, assignment)?;
    session.set("round", 0u32);
    Ok(Redirect::to("/solver").into_response())
}

pub async fn load_example_and_redirect(
    State(state): State<AppState>,
    session: Session<SessionNullPool>,
    form: axum::extract::Form<ExampleParams>,
) -> Result<Response, util::AppError> {
    let example_name = &form.example_name;

    let example = EXAMPLES
        .iter()
        .find(|e| e.name == *example_name)
        .context(format!("Example '{example_name}' not found"))?;

    let temp_dir = tempfile::Builder::new()
        .prefix(".demystify-")
        .tempdir_in(".")
        .context("Failed to create temporary directory")?;

    let model_dest = temp_dir.path().join("upload.eprime");
    std::fs::write(&model_dest, example.model).context("Failed to write model file")?;

    let param_dest = temp_dir.path().join("upload.param");
    std::fs::write(&param_dest, example.param).context("Failed to write param file")?;

    load_model(
        &session,
        &state,
        temp_dir,
        Some("upload.eprime".into()),
        Some("upload.param".into()),
        None,
    )?;
    session.set("round", 0u32);
    Ok(Redirect::to("/solver").into_response())
}

pub async fn preview_example(
    State(state): State<AppState>,
    form: axum::extract::Form<ExampleParams>,
) -> Result<Html<String>, util::AppError> {
    let example_name = &form.example_name;

    let example = EXAMPLES
        .iter()
        .find(|e| e.name == *example_name)
        .context(format!("Example '{example_name}' not found"))?;

    let mut ctx = tera::Context::new();
    ctx.insert("example_name", example_name);
    ctx.insert("description", example.description);
    ctx.insert("param_content", example.param);

    let html = state.tera.render("partials/param_editor.html", &ctx)?;
    Ok(Html(html))
}

pub async fn submit_example(
    State(state): State<AppState>,
    session: Session<SessionNullPool>,
    form: axum::extract::Form<SubmitExampleParams>,
) -> Result<Response, util::AppError> {
    let example_name = &form.example_name;

    let example = EXAMPLES
        .iter()
        .find(|e| e.name == *example_name)
        .context(format!("Example '{example_name}' not found"))?;

    let temp_dir = tempfile::Builder::new()
        .prefix(".demystify-")
        .tempdir_in(".")
        .context("Failed to create temporary directory")?;

    let model_dest = temp_dir.path().join("upload.eprime");
    std::fs::write(&model_dest, example.model).context("Failed to write model file")?;

    let param_dest = temp_dir.path().join("upload.param");
    std::fs::write(&param_dest, &form.param_content).context("Failed to write param file")?;

    load_model(
        &session,
        &state,
        temp_dir,
        Some("upload.eprime".into()),
        Some("upload.param".into()),
        None,
    )?;
    session.set("round", 0u32);
    Ok(Redirect::to("/solver").into_response())
}

fn load_model(
    session: &Session<SessionNullPool>,
    state: &AppState,
    temp_dir: tempfile::TempDir,
    model: Option<PathBuf>,
    param: Option<PathBuf>,
    assignment: Option<PathBuf>,
) -> anyhow::Result<()> {
    let model_path = temp_dir.path().join(model.unwrap());
    let param_path = temp_dir.path().join(param.unwrap());

    let puz = if let Some(assignment) = assignment {
        // Mystify-style upload: model declares clue cells as `find` variables
        // and the assignment file pins them.  Accept either a bare assignment
        // object or a full mystify output JSON.
        let assignment_path = temp_dir.path().join(assignment);
        let raw: serde_json::Value =
            serde_json::from_reader(std::io::BufReader::new(File::open(&assignment_path)?))
                .context("assignment file is not valid JSON")?;
        let assignment_obj = if raw.get("puzzle").is_some() {
            problem::parse::mystify_puzzle_assignment(&raw)?.clone()
        } else {
            raw
        };
        problem::parse::parse_essence_with_assignment(
            &model_path,
            &param_path,
            &assignment_obj,
            Default::default(),
        )?
    } else {
        let puzzle = problem::parse::parse_essence(&model_path, &param_path)?;
        PuzzleSolver::new(Arc::new(puzzle))?
    };

    let plan = PuzzlePlanner::new(puz).with_database(state.strategy_db.clone());
    set_solver_global(session, plan);
    Ok(())
}

// ─── Export / import ───

pub async fn solver_export(session: Session<SessionNullPool>) -> Result<Response, util::AppError> {
    let solver = get_solver_global(&session)?;
    let solver = solver.lock().unwrap();

    let snapshot = solver.export_snapshot()?;
    let json = serde_json::to_string_pretty(&snapshot).context("Failed to serialize session")?;

    Ok(Response::builder()
        .header("Content-Type", "application/json")
        .header(
            "Content-Disposition",
            "attachment; filename=\"demystify-session.json\"",
        )
        .body(Body::from(json))
        .unwrap())
}

pub async fn solver_import(
    State(state): State<AppState>,
    session: Session<SessionNullPool>,
    mut multipart: Multipart,
) -> Result<Response, util::AppError> {
    let mut json_data: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .context("Failed to parse multipart upload")?
    {
        let name = field.name().unwrap_or("");
        if name == "session_file" {
            let data = field.bytes().await.context("Failed to read file bytes")?;
            json_data = Some(data.to_vec());
        }
    }

    let json_data = json_data.context("No session file uploaded")?;
    let snapshot: demystify::snapshot::SessionSnapshot =
        serde_json::from_slice(&json_data).context("Invalid session JSON")?;

    let solver_session = util::import_snapshot_into_session(snapshot, state.strategy_db.clone())?;
    let round = solver_session.history.len().saturating_sub(1) as u32;

    set_solver_global_session(&session, solver_session);
    session.set("round", round);

    Ok(Redirect::to("/solver").into_response())
}

// ─── Legacy compatibility: old JSON full-solve endpoint ───

pub async fn dump_full_solve(
    session: Session<SessionNullPool>,
) -> Result<axum::Json<serde_json::Value>, util::AppError> {
    let solver = get_solver_global(&session)?;
    let mut solver = solver.lock().unwrap();
    let solve = solver.planner.quick_solve();
    Ok(axum::Json(serde_json::value::to_value(solve).unwrap()))
}

// ─── Solve tree ───

pub async fn solvetree_page(
    State(state): State<AppState>,
    session: Session<SessionNullPool>,
) -> Result<Response, util::AppError> {
    if get_solver_global(&session).is_err() {
        let mut ctx = tera::Context::new();
        ctx.insert("view", "solvetree");
        let html = state.tera.render("no_puzzle.html", &ctx)?;
        return Ok(Html(html).into_response());
    }

    let mut ctx = tera::Context::new();
    ctx.insert("view", "solvetree");
    let html = state.tera.render("solvetree.html", &ctx)?;
    Ok(Html(html).into_response())
}

#[derive(Deserialize)]
pub struct SolveTreeParams {
    merge_strategy: Option<String>,
    merge_mus_size: Option<usize>,
}

pub async fn solvetree_build(
    session: Session<SessionNullPool>,
    form: axum::extract::Form<SolveTreeParams>,
) -> Result<axum::Json<serde_json::Value>, util::AppError> {
    let solver = get_solver_global(&session)?;
    let puzzle = solver.lock().unwrap().planner.puzzle_arc();

    let merge_strategy = match form.merge_strategy.as_deref() {
        Some("greedy") => MergeStrategy::Greedy,
        Some("minimal") => MergeStrategy::Minimal,
        _ => MergeStrategy::None,
    };

    let config = SolveTreeConfig {
        merge_strategy,
        merge_mus_size: form.merge_mus_size.unwrap_or(1),
        ..SolveTreeConfig::default()
    };

    let tree = tokio::task::spawn_blocking(move || SolveTree::build(puzzle, &config))
        .await
        .context("Tree build task panicked")??;

    let json = tree.to_d3_json();
    Ok(axum::Json(serde_json::to_value(json)?))
}

/// Dev-only: ask the server to exit.  Pair with a shell loop
/// (`while true; do cargo run -p demystify-web --bin demystify-web; done`)
/// to get an in-place restart after code changes.  Not linked from the UI.
pub async fn quit() -> impl IntoResponse {
    eprintln!("/quit hit — exiting so the launcher can restart the server.");
    // Spawn the exit so the response has a chance to flush.
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        std::process::exit(0);
    });
    "Restarting server.\n"
}
