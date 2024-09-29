use anyhow::{bail, Result};
use tracing::{trace, Level};

fn main() -> Result<()> {
    tracing_subscriber::fmt::Subscriber::builder()
        .with_max_level(Level::TRACE)
        .init();

    let x = Vec::from_iter(1..100);
    let y = x
        .into_iter()
        .map(|x| {
            trace!("x: {}", x);
            if x > 10 {
                bail!("lol")
            } else {
                Ok(x)
            }
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(())
}
