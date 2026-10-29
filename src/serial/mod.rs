pub mod port;
pub mod daemon;
pub mod protocol;
pub mod spawn;

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

/// Text form of a `poke` reply.
///
/// An empty reply is a normal outcome (the device did not answer within
/// `--wait`), so it is named rather than rendered as nothing.
fn render_reply(reply: &[u8], hex: bool) -> String {
    if reply.is_empty() {
        return "(no reply)".to_string();
    }
    if hex {
        protocol::encode_hex(reply)
    } else {
        String::from_utf8_lossy(reply).into_owned()
    }
}

pub fn handle(serial: &super::SerialCommands, format: OutputFormat) -> anyhow::Result<()> {    match serial {
        super::SerialCommands::Scan => {
            let ports = port::scan()?;
            let headers = vec![
                "Port".into(),
                "Name".into(),
            ];
            let rows: Vec<Vec<String>> = ports
                .iter()
                .map(|p| {
                    vec![
                        p.name.clone(),
                        p.description.clone().unwrap_or_else(|| "-".into()),
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
            raw,
            baud,
            data_bits,
            parity,
            stop_bits,
        } => {
            let bytes = if *hex {
                protocol::decode_hex(data)?
            } else if *raw {
                data.as_bytes().to_vec()
            } else {
                protocol::unescape(data)?
            };
            let config = build_config(*baud, data_bits, parity, stop_bits);
            port::send(port, &bytes, &config)?;
            output::display(&OutputValue::Message("ok".into()), format);
        }
        super::SerialCommands::Poke {
            port,
            data,
            wait,
            hex,
            raw,
            show_hex,
            baud,
            data_bits,
            parity,
            stop_bits,
        } => {
            let bytes = if *hex {
                protocol::decode_hex(data)?
            } else if *raw {
                data.as_bytes().to_vec()
            } else {
                protocol::unescape(data)?
            };
            let config = build_config(*baud, data_bits, parity, stop_bits);
            let reply = port::poke(port, &bytes, *wait, &config)?;
            output::display(
                &OutputValue::Message(render_reply(&reply, *show_hex)),
                format,
            );
        }
        super::SerialCommands::Recv {
            port,
            timeout,
            show_hex,
            baud,
            data_bits,
            parity,
            stop_bits,
        } => {
            let config = build_config(*baud, data_bits, parity, stop_bits);
            let received = port::recv(port, *timeout, &config)?;
            if received.is_empty() {
                anyhow::bail!("no data received on {port} within {timeout} ms");
            }
            let text = if *show_hex {
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
            ttl,
            idle_timeout,
        } => {
            let config = build_config(*baud, data_bits, parity, stop_bits);
            let daemon_id =
                daemon::start(port, *baud, id.as_deref(), &config, *ttl, *idle_timeout)?;
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
                    "Limits".into(),
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
                            d.limits_display(),
                            d.started_at.clone(),
                        ]
                    })
                    .collect();
                output::display(&OutputValue::Table { headers, rows }, format);
            }
        }
        super::DaemonCommands::Send { id, data, hex, raw } => {
            let bytes = if *hex {
                protocol::decode_hex(data)?
            } else if *raw {
                data.as_bytes().to_vec()
            } else {
                protocol::unescape(data)?
            };
            daemon::send(id, &bytes)?;
            output::display(&OutputValue::Message("ok".into()), format);
        }
        super::DaemonCommands::Poke {
            id,
            data,
            wait,
            repeat,
            interval,
            show_hex,
        } => {
            let bytes = protocol::decode_hex(data)?;
            let repeats = (*repeat).max(1);
            let mut replies: Vec<String> = Vec::new();

            for i in 0..repeats {
                if i > 0 && *interval > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(*interval));
                }
                let reply = daemon::poke(id, &bytes, *wait)?;
                replies.push(render_reply(&reply, *show_hex));
            }

            // One cycle reads like `read`; several need to stay distinguishable.
            if replies.len() == 1 {
                output::display(&OutputValue::Message(replies.remove(0)), format);
            } else {
                output::display(&OutputValue::List(replies), format);
            }
        }
        super::DaemonCommands::Read {
            id,
            show_hex,
        } => {
            let received = daemon::read(id)?;
            if received.is_empty() {
                output::display(&OutputValue::Message("(no new data)".into()), format);
            } else {
                let text = if *show_hex {
                    protocol::encode_hex(&received)
                } else {
                    String::from_utf8_lossy(&received).into_owned()
                };
                output::display(&OutputValue::Message(text), format);
            }
        }
        super::DaemonCommands::History {
            id,
            limit,
            clear,
        } => {
            if *clear {
                daemon::history_clear(id)?;
                output::display(&OutputValue::Message("ok".into()), format);
            } else {
                let entries = daemon::history(id, *limit)?;
                if entries.is_empty() {
                    output::display(&OutputValue::Message("(no history)".into()), format);
                } else {
                    let headers = vec![
                        "Time".into(),
                        "Dir".into(),
                        "Bytes".into(),
                        "Data (hex)".into(),
                    ];
                    let rows: Vec<Vec<String>> = entries
                        .iter()
                        .map(|e| {
                            let data_display = if e.truncated {
                                format!("{}...(truncated)", e.hex)
                            } else {
                                e.hex.clone()
                            };
                            vec![
                                e.ts.clone(),
                                e.dir.clone(),
                                e.len.to_string(),
                                data_display,
                            ]
                        })
                        .collect();
                    output::display(&OutputValue::Table { headers, rows }, format);
                }
            }
        }
        super::DaemonCommands::Stop { id } => {
            daemon::stop(id)?;
            output::display(&OutputValue::Message("ok".into()), format);
        }
    }
    Ok(())
}
