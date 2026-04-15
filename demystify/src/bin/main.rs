use clap::Parser;
use demystify::{
    problem::{
        self,
        planner::{PlannerConfig, PuzzlePlanner},
        solver::{MusConfig, PuzzleSolver, SolverConfig},
        util::exec::{RunMethod, set_run_method},
    },
    stats::{print_mus_stats, print_sat_stats},
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

    #[arg(long, help = "Stop after this many solve steps")]
    max_steps: Option<usize>,

    #[arg(
        long,
        help = "Find all MUSes (disables find_one optimization that stops after the first MUS per batch)"
    )]
    all_muses: bool,

    #[arg(long, help = "Suppress progress and statistics output on stderr")]
    quiet: bool,

    #[arg(
        long,
        help = "Emit each solve step as a JSON object (instead of the default debug format)"
    )]
    json: bool,

    #[arg(
        long,
        value_enum,
        help = "Specify the method to run the solver (Native, Docker, Podman)"
    )]
    conjure: Option<RunMethod>,

    #[arg(
        long,
        help = "Save the parsed puzzle to a JSON file (for use with Lua interface or to skip parsing later)"
    )]
    save_parsed: Option<PathBuf>,

    #[arg(
        long,
        help = "Load a pre-parsed puzzle from JSON instead of parsing .eprime/.param files"
    )]
    load_parsed: Option<PathBuf>,

    #[arg(
        long,
        default_value_t = 1_000_000,
        help = "Per-SAT-call conflict limit (0 = no limit). Default 1,000,000."
    )]
    conflict_limit: i64,
}

fn main() -> anyhow::Result<()> {
    let opt = Opt::parse();

    let (non_block, _guard) = tracing_appender::non_blocking(File::create("demystify.trace")?);

    // Choose how we run conjure, native, docker or podman
    if let Some(method) = opt.conjure {
        set_run_method(method);
    }

    // Apply user-requested conflict limit (0 means "no limit"; the satcore
    // module honours non-positive values via clear_conflict_limit).
    demystify::satcore::set_global_conflict_limit(opt.conflict_limit);

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

    // Load puzzle either from pre-parsed JSON or by parsing .eprime/.param files
    let puzzle = if let Some(ref load_path) = opt.load_parsed {
        eprintln!("Loading pre-parsed puzzle from {:?}", load_path);
        problem::parse::PuzzleParse::load_from_json(load_path)?
    } else {
        problem::parse::parse_essence(&PathBuf::from(&opt.model), &PathBuf::from(&opt.param))?
    };

    // Save parsed puzzle to JSON if requested
    if let Some(ref save_path) = opt.save_parsed {
        eprintln!("Saving parsed puzzle to {:?}", save_path);
        puzzle.save_to_json(save_path)?;
        eprintln!("Saved successfully.");
    }

    let puzzle = Arc::new(puzzle);

    let solver = PuzzleSolver::new_with_config(
        puzzle,
        SolverConfig {
            only_assignments: opt.only_assign,
        },
    )?;

    let mus_config: MusConfig = {
        let mut cfg = if let Some(searches) = opt.searches {
            MusConfig::new_with_repeats(searches)
        } else {
            MusConfig::default()
        };
        if opt.all_muses {
            cfg.find_one = false;
        }
        cfg
    };

    let planner_config = PlannerConfig {
        mus_config,
        merge_small_threshold: opt.merge,
        skip_small_threshold: opt.skip,
        expand_to_all_deductions: true,
        max_steps: opt.max_steps,
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
    } else if opt.json {
        for step in planner.quick_solve() {
            let json_step: Vec<serde_json::Value> = step
                .iter()
                .map(|(lits, constraints)| {
                    serde_json::json!({
                        "lits": lits,
                        "constraints": constraints,
                    })
                })
                .collect();
            println!("{}", serde_json::to_string(&json_step).unwrap());
        }
    } else {
        let solve_fn: Box<dyn FnOnce() -> Vec<_>> = if opt.quiet {
            Box::new(|| planner.quick_solve())
        } else {
            Box::new(|| planner.quick_solve_with_progress())
        };
        for p in solve_fn() {
            println!("{p:?}");
        }
    }

    if !opt.quiet {
        print_mus_stats();
        print_sat_stats();
        demystify::satcore::print_phase_breakdown();
    }

    Ok(())
}
