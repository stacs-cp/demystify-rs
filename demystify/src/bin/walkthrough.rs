//! `demystify-walkthrough` — run a TOML walkthrough script and emit a
//! single self-contained HTML page.  See `demystify::walkthrough` for the
//! schema and executor.

use std::{fs, path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use clap::Parser;

use demystify::{
    named_strategy,
    problem::{
        self,
        planner::PuzzlePlanner,
        solver::{PuzzleSolver, SolverConfig},
        util::exec::{RunMethod, set_run_method},
    },
    walkthrough::{self, Script, compose_page},
};

#[derive(Parser, Debug)]
#[command(about = "Run a TOML walkthrough script and emit a static HTML page.")]
struct Opt {
    /// Path to the TOML walkthrough script.
    #[arg(long)]
    script: PathBuf,

    /// Where to write the rendered HTML page.
    #[arg(long, default_value = "walkthrough.html")]
    out: PathBuf,

    /// Override the script's `model` path (rarely needed).
    #[arg(long)]
    model: Option<PathBuf>,

    /// Override the script's `param` path (rarely needed).
    #[arg(long)]
    param: Option<PathBuf>,

    /// How to invoke the Conjure / SavileRow toolchain.
    #[arg(long)]
    conjure: Option<RunMethod>,

    /// Resolve relative `model`/`param` paths in the script against this
    /// directory instead of the script's own directory.  Useful when running
    /// scripts that reference paths from a project root.
    #[arg(long)]
    base_dir: Option<PathBuf>,

    /// Path to the named-strategy database directory.  Defaults to a search
    /// of standard locations relative to the current directory; absent that,
    /// runs with an empty database (MUSes render without strategy labels).
    #[arg(long)]
    strategy_db: Option<PathBuf>,
}

fn main() -> Result<()> {
    let opt = Opt::parse();

    if let Some(method) = opt.conjure {
        set_run_method(method);
    }

    let script_text = fs::read_to_string(&opt.script)
        .with_context(|| format!("reading script {}", opt.script.display()))?;
    let mut script: Script = toml::from_str(&script_text)
        .with_context(|| format!("parsing TOML script {}", opt.script.display()))?;

    if let Some(p) = opt.model {
        script.model = p;
    }
    if let Some(p) = opt.param {
        script.param = p;
    }

    // Resolve relative paths against the script's directory unless --base-dir
    // is given.  Authors writing scripts alongside their .eprime/.param files
    // get the obvious behaviour without futzing with absolute paths.
    let base = opt
        .base_dir
        .clone()
        .or_else(|| opt.script.parent().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    let model = if script.model.is_absolute() {
        script.model.clone()
    } else {
        base.join(&script.model)
    };
    let param = if script.param.is_absolute() {
        script.param.clone()
    } else {
        base.join(&script.param)
    };

    if let Some(limit) = script.conflict_limit {
        demystify::satcore::set_global_conflict_limit(limit);
    }

    eprintln!(
        "[walkthrough] model={} param={}",
        model.display(),
        param.display()
    );

    let puzzle = problem::parse::parse_essence(&model, &param)?;
    let puzzle = Arc::new(puzzle);
    let solver = PuzzleSolver::new_with_config(
        puzzle,
        SolverConfig {
            only_assignments: script.only_assignments.unwrap_or(false),
        },
    )?;
    let strategy_db = named_strategy::load_or_discover(opt.strategy_db.as_deref())?;
    let mut planner = PuzzlePlanner::new(solver).with_database(strategy_db);

    let mut executor = walkthrough::Executor::new(&mut planner);
    executor.run(&script)?;
    let sections = executor.into_sections();

    let title = script
        .title
        .clone()
        .or_else(|| {
            opt.script
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "Walkthrough".to_string());

    let html = compose_page(&title, &sections);
    fs::write(&opt.out, html).with_context(|| format!("writing {}", opt.out.display()))?;
    eprintln!(
        "[walkthrough] wrote {} ({} section{})",
        opt.out.display(),
        sections.len(),
        if sections.len() == 1 { "" } else { "s" }
    );
    Ok(())
}
