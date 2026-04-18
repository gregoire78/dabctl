use std::io::IsTerminal;

use anyhow::Result;
use tracing_subscriber::EnvFilter;

pub fn init(silent: bool) -> Result<()> {
    if silent {
        return Ok(());
    }

    let filter = match EnvFilter::try_from_default_env() {
        Ok(filter) => filter,
        Err(_) => EnvFilter::new("info"),
    };

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_ansi(std::io::stderr().is_terminal())
        .with_writer(std::io::stderr)
        .compact()
        .try_init();

    Ok(())
}
