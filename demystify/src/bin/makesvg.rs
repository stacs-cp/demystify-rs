use clap::Parser;
use demystify_lib::json::Problem;
use demystify_lib::svg::PuzzleDraw; // Add the missing import statement
use serde_json; // Add the missing import statement
use std::io::Write;
use std::{fs::File, path::PathBuf, process::exit};
use tracing::Level;
use tracing_subscriber::fmt::format::FmtSpan; // Add the missing import statement

#[derive(clap::Parser, Debug)]
struct Opt {
    #[arg(long)]
    puzzle: String,

    #[arg(long)]
    svg: String,

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

    let file = File::open(&opt.puzzle)?;
    let problem: Problem = serde_json::from_reader(file)?;

    let puz_draw = PuzzleDraw::new();

    let svg = puz_draw.draw_puzzle(&problem);

    let mut output_file = File::create(&opt.svg)?;
    let svg_string = svg.to_string();
    output_file.write_all(svg_string.as_bytes())?;
    Ok(())
}
