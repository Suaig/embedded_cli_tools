pub mod port;
pub mod daemon;
pub mod protocol;

use crate::output::{self, OutputFormat, OutputValue};
use port::{SerialConfig};

/// Convert CLI SerialDataBits enum to serialport::DataBits
fn to_data_bits(val: &super::SerialDataBits) -> serialport::DataBits {
    match val {
        super::SerialDataBits::Bits5 => serialport::DataBits::Five,
        super::SerialDataBits::Bits6 => serialport::DataBits::Six,
        super::SerialDataBits::Bits7 => serialport::DataBits::Seven,
        super::SerialDataBits::Bits8 => serialport::DataBits::Eight,
    }
}

/// Convert CLI SerialParity enum to serialport::Parity
fn to_parity(val: &super::SerialParity) -> serialport::Parity {
    match val {
        super::SerialParity::None => serialport::Parity::None,
        super::SerialParity::Odd => serialport::Parity::Odd,
        super::SerialParity::Even => serialport::Parity::Even,
    }
}

/// Convert CLI SerialStopBits enum to serialport::StopBits
fn to_stop_bits(val: &super::SerialStopBits) -> serialport::StopBits {
    match val {
        super::SerialStopBits::Bits1 => serialport::StopBits::One,
        super::SerialStopBits::Bits2 => serialport::StopBits::Two,
    }
}

/// Build SerialConfig from CLI arguments
fn build_config(
    baud: u32,
    data_bits: &super::SerialDataBits,
    parity: &super::SerialParity,
    stop_bits: &super::SerialStopBits,
) -> SerialConfig {
    SerialConfig::new(baud, to_data_bits(data_bits), to_parity(parity), to_stop_bits(stop_bits))
}

pub fn handle(serial: &super::SerialCommands, format: OutputFormat) -> anyhow::Result<()> {
    match serial {
        super::SerialCommands::Scan => {
            let ports = port::scan()?;
            let headers = vec![
                "Port".into(),
                "Type".into(),
                "Manufacturer".into(),
                "VID".into(),
                "PID".into(),
                "Serial".into(),
            ];
            let rows: Vec<Vec<String>> = ports
                .iter()
                .map(|p| {
                    vec![
                        p.name.clone(),
                        p.port_type.clone(),
                        p.manufacturer.clone().unwrap_or_else(|| "-".into()),
                        p.vid.map(|v| format!("0x{:04X}", v)).unwrap_or_else(|| "-".into()),
                        p.pid.map(|v| format!("0x{:04X}", v)).unwrap_or_else(|| "-".into()),
                        p.serial_number.clone().unwrap_or_else(|| "-".into()),
                    ]
                })
                .collect();
            let output = if rows.is_empty() {
                OutputValue::Message("No serial ports found".into())
            } else {
                OutputValue::Table { headers, rows }
            };
            output::display(&output, format);
        }
        super::SerialCommands::Send {
            port,
            data,
            hex,
            baud,
            data_bits,
            parity,
            stop_bits,
        } => {
            let bytes = if *hex {
                protocol::decode_hex(data)?
            } else {
                data.as_bytes().to_vec()
            };
            let config = build_config(*baud, data_bits, parity, stop_bits);
            port::send(port, &bytes, &config)?;
            output::display(&OutputValue::Message("ok".into()), format);
        }
        super::SerialCommands::Recv {
            port,
            timeout,
            hex,
            baud,
            data_bits,
            parity,
            stop_bits,
        } => {
            let config = build_config(*baud, data_bits, parity, stop_bits);
            let received = port::recv(port, *timeout, &config)?;
            let text = if *hex {
                protocol::encode_hex(&received)
            } else {
                String::from_utf8_lossy(&received).into_owned()
            };
            output::display(&OutputValue::Message(text), format);
        }
        super::SerialCommands::Daemon { command } => {
            handle_daemon(command, format)?;
        }
    }
    Ok(())
}

fn handle_daemon(daemon: &super::DaemonCommands, format: OutputFormat) -> anyhow::Result<()> {
    match daemon {
        super::DaemonCommands::Start {
            port,
            baud,
            id,
            data_bits,
            parity,
            stop_bits,
        } => {
            let config = build_config(*baud, data_bits, parity, stop_bits);
            let daemon_id = daemon::start(port, *baud, id.as_deref(), &config)?;
            output::display(&OutputValue::Message(daemon_id), format);
        }
        super::DaemonCommands::List => {
            let daemons = daemon::list()?;
            if daemons.is_empty() {
                output::display(&OutputValue::Message("No daemons found".into()), format);
            } else {
                let headers = vec![
                    "ID".into(),
                    "Port".into(),
                    "Baud".into(),
                    "PID".into(),
                    "Status".into(),
                    "Started".into(),
                ];
                let rows: Vec<Vec<String>> = daemons
                    .iter()
                    .map(|d| {
                        vec![
                            d.id.clone(),
                            d.port_name.clone(),
                            d.baud_rate.to_string(),
                            d.pid.to_string(),
                            d.status.clone(),
                            d.started_at.clone(),
                        ]
                    })
                    .collect();
                output::display(&OutputValue::Table { headers, rows }, format);
            }
        }
        super::DaemonCommands::Send { id, data, hex } => {
            let bytes = if *hex {
                protocol::decode_hex(data)?
            } else {
                data.as_bytes().to_vec()
            };
            daemon::send(id, &bytes)?;
            output::display(&OutputValue::Message("ok".into()), format);
        }
        super::DaemonCommands::Read {
            id,
            timeout: _,
            hex,
            clear,
        } => {
            let received = daemon::read(id, *clear)?;
            let text = if *hex {
                protocol::encode_hex(&received)
            } else {
                String::from_utf8_lossy(&received).into_owned()
            };
            output::display(&OutputValue::Message(text), format);
        }
        super::DaemonCommands::Stop { id } => {
            daemon::stop(id)?;
            output::display(&OutputValue::Message("ok".into()), format);
        }
    }
    Ok(())
}
