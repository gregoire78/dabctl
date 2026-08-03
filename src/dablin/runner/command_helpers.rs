use anyhow::Result;
use std::io::{BufReader, Read};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use super::{init_logger, open_eti_reader, setup_ctrlc};

pub(super) type CommandReader = BufReader<Box<dyn Read>>;
pub(super) type CommandRuntime = Arc<AtomicBool>;
pub(super) type CommandBootstrap = (CommandRuntime, CommandReader);

pub(super) fn init_command_input(silent: bool, input: &str) -> Result<CommandBootstrap> {
    init_logger(silent);
    let running = setup_ctrlc();
    let reader = open_eti_reader(input)?;
    Ok((running, reader))
}

pub(super) fn run_with_command_input<F>(silent: bool, input: &str, run: F) -> Result<()>
where
    F: FnOnce(&CommandRuntime, &mut CommandReader) -> Result<()>,
{
    let (running, mut reader) = init_command_input(silent, input)?;
    run(&running, &mut reader)
}
