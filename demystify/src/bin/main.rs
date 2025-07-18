use clap::Parser;
use demystify::{
    problem::{
        self,
        planner::{PlannerConfig, PuzzlePlanner},
        solver::{MusConfig, PuzzleSolver, SolverConfig},
        util::exec::{RunMethod, set_run_method},
    },
    web::{base_css, base_javascript},
};
use std::{fs::File, path::PathBuf, sync::Arc};
use tracing::Level;
use tracing_subscriber::fmt::format::FmtSpan;

#[derive(clap::Parser, Debug)]
struct Opt {
    #[arg(long)]
    model: String,

    #[arg(long)]
    param: String,

    #[arg(
        long,
        default_value_t = 1,
        help = "Merge MUSes of this size or smaller together in a single step (set to -1 to disable)"
    )]
    merge: i64,

    #[arg(
        long,
        default_value_t = 0,
        help = "Skip MUSes of this size or smaller (set to -1 to disable)"
    )]
    skip: i64,

    #[arg(long)]
    trace: bool,

    #[arg(long)]
    html: bool,

    #[arg(long)]
    only_assign: bool,

    #[arg(long)]
    searches: Option<i64>,

    #[arg(
        long,
        value_enum,
        help = "Specify the method to run the solver (Native, Docker, Podman)"
    )]
    conjure: Option<RunMethod>,
}

fn main() -> anyhow::Result<()> {
    let opt = Opt::parse();

    let (non_block, _guard) = tracing_appender::non_blocking(File::create("demystify.trace")?);

    // Choose how we run conjure, native, docker or podman
    if let Some(method) = opt.conjure {
        set_run_method(method);
    }

    if opt.trace {
        tracing_subscriber::fmt()
            .with_span_events(FmtSpan::ACTIVE)
            .with_max_level(Level::TRACE)
            //.with_env_filter("trace,tracer=off")
            .with_ansi(false)
            .without_time()
            //.pretty()
            .with_writer(non_block)
            .init();
    }

    let puzzle =
        problem::parse::parse_essence(&PathBuf::from(opt.model), &PathBuf::from(opt.param))?;

    let puzzle = Arc::new(puzzle);

    let solver = PuzzleSolver::new_with_config(
        puzzle,
        SolverConfig {
            only_assignments: opt.only_assign,
        },
    )?;

    let mus_config: MusConfig = if let Some(searches) = opt.searches {
        MusConfig::new_with_repeats(searches)
    } else {
        MusConfig::default()
    };

    let planner_config = PlannerConfig {
        mus_config,
        merge_small_threshold: opt.merge,
        skip_small_threshold: opt.skip,
        expand_to_all_deductions: true,
    };

    let mut planner = PuzzlePlanner::new_with_config(solver, planner_config);

    if opt.html {
        let html = planner.quick_solve_html();
        println!(
            "<html> <head> <style> {} </style> <script> {} </script> </head>",
            base_css(),
            base_javascript()
        );
        println!("<body> {html}");
        println!("<script> doJavascript(); </script>");
        println!("</body> </html>");
    } else {
        for p in planner.quick_solve_with_progress() {
            println!("{p:?}");
        }
    }

    Ok(())
}
