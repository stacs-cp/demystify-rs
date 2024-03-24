use demystify_lib::problem;
use std::fs::File;
use tracing::Level;
use tracing_subscriber::fmt::format::FmtSpan; // Add the missing import statement

fn main() -> anyhow::Result<()> {
    let (non_block, _guard) = tracing_appender::non_blocking(File::create("demystify.trace")?);

    if true {
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

    let _ = problem::parse::parse_essence("eprime/binairo.eprime", "eprime/binairo-1.param")?;

    Ok(())
}
