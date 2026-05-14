pub mod openocd;
pub mod keil_debug;

use crate::output::{self, OutputFormat};

pub fn handle(debug: &super::DebugCommands, format: OutputFormat) -> anyhow::Result<()> {
    match debug {
        super::DebugCommands::Openocd { args } => {
            let _ = args;
            output::not_implemented("debug openocd", format);
        }
        super::DebugCommands::Keil { path, target } => {
            let _ = (path, target);
            output::not_implemented("debug keil", format);
        }
    }
    Ok(())
}
