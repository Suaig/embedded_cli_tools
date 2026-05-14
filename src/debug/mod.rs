pub mod openocd;
pub mod keil_debug;

use crate::output::{self, OutputFormat, OutputValue};

pub fn handle(debug: &super::DebugCommands, format: OutputFormat) -> anyhow::Result<()> {
    match debug {
        super::DebugCommands::Openocd { args } => {
            let _ = args;
            let msg = "OpenOCD support not yet implemented. Planned for future release.";
            output::display(&OutputValue::Message(msg.to_string()), format);
        }
        super::DebugCommands::Keil { path, target } => {
            let target_display = target.as_deref().unwrap_or("default");
            let msg = format!(
                "Keil debug support not yet implemented. Would invoke: UV4 -d {} -t {}",
                path, target_display
            );
            output::display(&OutputValue::Message(msg), format);
        }
    }
    Ok(())
}
