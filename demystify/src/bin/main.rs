use clap::Parser;
use demystify::{
    problem::{
        self,
        planner::{MusMethod, PlannerConfig, PuzzlePlanner},
        solver::{MusConfig, PuzzleSolver, SolverConfig},
        util::exec::{RunMethod, set_run_method},
    },
    stats::{print_mus_stats, print_sat_stats},
    web::{base_css, base_javascript},
};
use std::{fs::File, path::PathBuf, sync::Arc};
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::prelude::*;

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

    #[arg(
        long,
        value_enum,
        default_value_t = MusMethod::default(),
        help = "MUS generation algorithm: core (raw cores only), mus (standard MUS search), core+mus (hybrid: size-1 pass, cores, then full MUS if needed)"
    )]
    mus_method: MusMethod,

    #[arg(
        long,
        value_delimiter = ',',
        help = "Enable logging targets on stderr (comma-separated). Available: progress, cores, solver, planner, solve, parser"
    )]
    log: Vec<String>,
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

    // Set up tracing: --trace writes everything to file, --log writes
    // selected targets to stderr.  Without --quiet, "progress" is
    // implicitly enabled on stderr.
    {
        let trace_layer = if opt.trace {
            Some(
                tracing_subscriber::fmt::layer()
                    .with_span_events(FmtSpan::ACTIVE)
                    .with_ansi(false)
                    .without_time()
                    .with_writer(non_block)
                    .with_filter(tracing_subscriber::filter::LevelFilter::TRACE),
            )
        } else {
            None
        };

        let mut log_targets = opt.log.clone();
        if !opt.quiet && !log_targets.contains(&"progress".to_string()) {
            log_targets.push("progress".to_string());
        }

        let log_layer = if !log_targets.is_empty() {
            let filter_str = log_targets
                .iter()
                .map(|t| format!("{}=info", t.trim()))
                .collect::<Vec<_>>()
                .join(",");
            Some(
                tracing_subscriber::fmt::layer()
                    .with_writer(std::io::stderr)
                    .with_target(false)
                    .with_level(false)
                    .without_time()
                    .with_filter(tracing_subscriber::filter::EnvFilter::new(filter_str)),
            )
        } else {
            None
        };

        tracing_subscriber::registry()
            .with(trace_layer)
            .with(log_layer)
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
        mus_method: opt.mus_method,
    };

    let mut planner = PuzzlePlanner::new_with_config(solver, planner_config);
    let t_solve = std::time::Instant::now();

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
        for p in planner.quick_solve() {
            println!("{p:?}");
        }
    }

    let solve_secs = t_solve.elapsed().as_secs_f64();
    tracing::info!(target: "progress", "solve completed in {solve_secs:.2}s");

    if !opt.quiet {
        print_mus_stats();
        print_sat_stats();
        demystify::satcore::print_phase_breakdown();
    }

    Ok(())
}
