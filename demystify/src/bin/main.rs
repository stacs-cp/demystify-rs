use clap::Parser;
use demystify_lib::problem::{self, planner::PuzzlePlanner, solver::PuzzleSolver};
use std::{fs::File, path::PathBuf, process::exit};
use tracing::Level;
use tracing_subscriber::fmt::format::FmtSpan;

#[derive(clap::Parser, Debug)]
struct Opt {
    #[arg(long)]
    model: String,

    #[arg(long)]
    param: String,

    #[arg(long)]
    quick: bool,

    #[arg(long)]
    trace: bool,
}

fn main() -> anyhow::Result<()> {
    let opt = Opt::parse();

    let (non_block, _guard) = tracing_appender::non_blocking(File::create("demystify.trace")?);

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

    let solver = PuzzleSolver::new(puzzle)?;

    let mut planner = PuzzlePlanner::new(solver);

    if opt.quick {
        for p in planner.quick_solve() {
            println!("{p:?}");
        }
        exit(0);
    }

    Ok(())
}
