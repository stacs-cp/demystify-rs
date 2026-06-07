use clap::Parser;
use demystify::json::Problem;
use demystify::web::puzsvg::PuzzleDraw;
use std::fs::File;
use std::io::Write;
use tracing::Level;
use tracing_subscriber::fmt::format::FmtSpan;

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

    // Only open the trace file when --trace is given, so an ordinary run
    // doesn't leave an empty demystify.trace in the working directory. The
    // flush guard is held until the end of main.
    let _trace_guard = if opt.trace {
        let (non_block, guard) = tracing_appender::non_blocking(File::create("demystify.trace")?);
        tracing_subscriber::fmt()
            .with_span_events(FmtSpan::ACTIVE)
            .with_max_level(Level::TRACE)
            .with_ansi(false)
            .without_time()
            .with_writer(non_block)
            .init();
        Some(guard)
    } else {
        None
    };

    let file = File::open(&opt.puzzle)?;
    let problem: Problem = serde_json::from_reader(file)?;

    let puz_draw = PuzzleDraw::new_with_decs(&problem.puzzle.kind, &problem.puzzle.decorations);

    let svg = puz_draw.draw_puzzle(&problem);

    let mut output_file = File::create(&opt.svg)?;
    let svg_string = svg.to_string();
    output_file.write_all(svg_string.as_bytes())?;
    Ok(())
}
