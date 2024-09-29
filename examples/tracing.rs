use anyhow::Result;
use tracing::{info, instrument, Level};
use tracing_subscriber::{fmt::format::FmtSpan, util::SubscriberInitExt};

#[instrument(level = "trace", ret)]
fn hi(x: usize) -> usize {
    println!("hi");
    info!("hi");
    1
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::Subscriber::builder()
        .with_span_events(FmtSpan::ENTER | FmtSpan::EXIT)
        .with_max_level(Level::TRACE)
        .init();

    _ = hi(4);

    Ok(())
}
