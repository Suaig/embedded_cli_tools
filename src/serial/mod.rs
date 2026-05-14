pub mod port;
pub mod daemon;
pub mod protocol;

use crate::output::{self, OutputFormat};

pub fn handle(serial: &super::SerialCommands, format: OutputFormat) -> anyhow::Result<()> {
    match serial {
        super::SerialCommands::Scan => {
            output::not_implemented("serial scan", format);
        }
        super::SerialCommands::Send { port, data, hex, baud, data_bits, parity, stop_bits } => {
            let _ = (port, data, hex, baud, data_bits, parity, stop_bits);
            output::not_implemented("serial send", format);
        }
        super::SerialCommands::Recv { port, timeout, hex, baud, data_bits, parity, stop_bits } => {
            let _ = (port, timeout, hex, baud, data_bits, parity, stop_bits);
            output::not_implemented("serial recv", format);
        }
        super::SerialCommands::Daemon { command } => {
            handle_daemon(command, format);
        }
    }
    Ok(())
}

fn handle_daemon(daemon: &super::DaemonCommands, format: OutputFormat) {
    match daemon {
        super::DaemonCommands::Start { port, baud, id, data_bits, parity, stop_bits } => {
            let _ = (port, baud, id, data_bits, parity, stop_bits);
            output::not_implemented("serial daemon start", format);
        }
        super::DaemonCommands::List => {
            output::not_implemented("serial daemon list", format);
        }
        super::DaemonCommands::Send { id, data, hex } => {
            let _ = (id, data, hex);
            output::not_implemented("serial daemon send", format);
        }
        super::DaemonCommands::Read { id, timeout, hex, clear } => {
            let _ = (id, timeout, hex, clear);
            output::not_implemented("serial daemon read", format);
        }
        super::DaemonCommands::Stop { id } => {
            let _ = id;
            output::not_implemented("serial daemon stop", format);
        }
    }
}
