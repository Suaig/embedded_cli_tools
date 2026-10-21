use serialport::{DataBits, Parity, StopBits};
use std::io::{Read, Write};
use std::time::Duration;

/// Information about a scanned serial port.
#[derive(Debug, Clone)]
pub struct PortInfo {
    pub name: String,
    /// Human-readable label: the WMI friendly name when available, otherwise the
    /// driver's manufacturer string.
    pub description: Option<String>,
}

/// Serial port configuration.
#[derive(Debug, Clone)]
pub struct SerialConfig {
    pub baud_rate: u32,
    pub data_bits: DataBits,
    pub parity: Parity,
    pub stop_bits: StopBits,
}

impl SerialConfig {
    pub fn new(baud_rate: u32, data_bits: DataBits, parity: Parity, stop_bits: StopBits) -> Self {
        Self {
            baud_rate,
            data_bits,
            parity,
            stop_bits,
        }
    }
}

/// The label to show for a port.
///
/// The WMI friendly name wins: the driver's manufacturer string alone
/// ("Prolific", "Microsoft") cannot tell two boards apart, and USB serial ports
/// — the ones this tool exists for — always have a manufacturer, so treating it
/// as a fallback for the friendly name means the friendly name is never shown.
fn port_label(friendly: Option<&str>, manufacturer: Option<&str>) -> Option<String> {
    friendly.or(manufacturer).map(ToOwned::to_owned)
}

/// Scan available serial ports.
/// Returns an empty list (not an error) if no ports are found.
pub fn scan() -> anyhow::Result<Vec<PortInfo>> {
    let ports = serialport::available_ports()?;
    let mut result: Vec<PortInfo> = ports
        .into_iter()
        .map(|p| PortInfo {
            name: p.port_name.clone(),
            description: match &p.port_type {
                serialport::SerialPortType::UsbPort(info) => info.manufacturer.clone(),
                _ => None,
            },
        })
        .collect();

    // On Windows, replace with the friendly name where one is known.
    #[cfg(target_os = "windows")]
    {
        let wmi_names = query_wmi_port_names();
        for port in &mut result {
            port.description = port_label(
                wmi_names.get(&port.name).map(String::as_str),
                port.description.as_deref(),
            );
        }
    }

    Ok(result)
}

/// Query serial port friendly names via PowerShell (Windows only).
#[cfg(target_os = "windows")]
fn query_wmi_port_names() -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();

    let ps_script = concat!(
        "[Console]::OutputEncoding=[Text.Encoding]::UTF8; ",
        "Get-CimInstance Win32_PnPEntity ",
        "| Where-Object { $_.Name -match '\\(COM' } ",
        "| ForEach-Object { $m=[regex]::Match($_.Name,'COM\\d+'); $m.Value+'|'+$_.Name }"
    );

    let output = match std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", ps_script])
        .output()
    {
        Ok(o) => o,
        Err(_) => return map,
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((com, name)) = line.split_once('|') {
            let com = com.trim().to_string();
            let name = name.trim().to_string();
            if !com.is_empty() && !name.is_empty() {
                map.insert(com, name);
            }
        }
    }

    map
}

/// The line settings carried on the daemon command line as `--data-bits`.
///
/// These are the actual bit counts, not the `DataBits` discriminants: the
/// discriminant of `DataBits::Eight` is 3, which the child process would read
/// back as neither 5, 6, 7 nor 8 and quietly fall back to 8 data bits.
pub fn data_bits_value(bits: DataBits) -> u8 {
    match bits {
        DataBits::Five => 5,
        DataBits::Six => 6,
        DataBits::Seven => 7,
        DataBits::Eight => 8,
    }
}

/// Inverse of `data_bits_value`; `None` for anything the parent never sends.
pub fn data_bits_from_value(value: u8) -> Option<DataBits> {
    match value {
        5 => Some(DataBits::Five),
        6 => Some(DataBits::Six),
        7 => Some(DataBits::Seven),
        8 => Some(DataBits::Eight),
        _ => None,
    }
}

/// The line settings carried on the daemon command line as `--stop-bits`.
pub fn stop_bits_value(bits: StopBits) -> u8 {
    match bits {
        StopBits::Two => 2,
        _ => 1,
    }
}

/// Inverse of `stop_bits_value`; `None` for anything the parent never sends.
pub fn stop_bits_from_value(value: u8) -> Option<StopBits> {
    match value {
        1 => Some(StopBits::One),
        2 => Some(StopBits::Two),
        _ => None,
    }
}

/// Turn a port-open failure into something actionable.
///
/// `serialport`'s own error is a bare OS string that never says which port was
/// meant, which is useless the moment more than one is in play. It also cannot
/// be made more specific than this: on Windows the crate folds
/// `ERROR_ACCESS_DENIED` (the port is busy) into `ErrorKind::NoDevice`, the
/// same kind as "no such port" - so the two cases are indistinguishable in
/// code, and the hint has to name both rather than guess one.
pub fn describe_open_error(port_name: &str, err: &serialport::Error) -> String {
    let mut msg = format!("cannot open {port_name}: {err}");
    if matches!(err.kind(), serialport::ErrorKind::NoDevice) {
        msg.push_str(
            " - either no such port, or something else already holds it (a running \
             `emb serial daemon`, Keil, or a serial terminal)",
        );
        // Port names drift: the CH340 that was COM20 last week is COM5 today,
        // and a stale name costs a round of guessing every time. Listing what
        // is actually attached turns that into a one-line fix.
        let present = present_ports_hint(port_name);
        if !present.is_empty() {
            msg.push_str(&format!("; ports present: {present}"));
        }
    }
    msg
}

/// "COM3 (Intel ...), COM5 (USB-SERIAL CH340)", excluding the port asked for.
fn present_ports_hint(port_name: &str) -> String {
    let Ok(ports) = scan() else {
        return String::new();
    };
    ports
        .iter()
        .filter(|p| !p.name.eq_ignore_ascii_case(port_name))
        .map(|p| match p.description.as_deref() {
            Some(d) if !d.is_empty() => format!("{} ({})", p.name, d),
            _ => p.name.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Send data to a serial port (open, send, close).
pub fn send(port_name: &str, data: &[u8], config: &SerialConfig) -> anyhow::Result<()> {
    let mut port = serialport::new(port_name, config.baud_rate)
        .data_bits(config.data_bits)
        .parity(config.parity)
        .stop_bits(config.stop_bits)
        .timeout(Duration::from_secs(5))
        .open()
        .map_err(|e| anyhow::anyhow!(describe_open_error(port_name, &e)))?;
    port.write_all(data)?;
    port.flush()?;
    Ok(())
}

/// Receive data from a serial port with timeout (open, recv, close).
pub fn recv(port_name: &str, timeout_ms: u64, config: &SerialConfig) -> anyhow::Result<Vec<u8>> {
    let mut port = serialport::new(port_name, config.baud_rate)
        .data_bits(config.data_bits)
        .parity(config.parity)
        .stop_bits(config.stop_bits)
        .timeout(Duration::from_millis(timeout_ms))
        .open()
        .map_err(|e| anyhow::anyhow!(describe_open_error(port_name, &e)))?;

    let mut buf = vec![0u8; 4096];
    let mut received = Vec::new();

    loop {
        match port.read(&mut buf) {
            Ok(n) => {
                received.extend_from_slice(&buf[..n]);
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {
                break;
            }
            Err(e) => {
                return Err(e.into());
            }
        }
    }

    Ok(received)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `emb serial daemon start` re-spawns itself and passes the line settings
    /// on the command line. Carrying the enum discriminant here (`Eight as u8`
    /// is 3) made the child fall back to 8 data bits whatever was requested.
    #[test]
    fn data_bits_survive_the_daemon_command_line() {
        for bits in [
            DataBits::Five,
            DataBits::Six,
            DataBits::Seven,
            DataBits::Eight,
        ] {
            let value = data_bits_value(bits);
            let round_tripped = data_bits_from_value(value)
                .unwrap_or_else(|| panic!("{value} is not a value the parent ever sends"));
            assert_eq!(
                data_bits_value(round_tripped),
                value,
                "data bits {value} did not survive the round trip"
            );
        }
    }

    #[test]
    fn stop_bits_survive_the_daemon_command_line() {
        for bits in [StopBits::One, StopBits::Two] {
            let value = stop_bits_value(bits);
            let round_tripped = stop_bits_from_value(value)
                .unwrap_or_else(|| panic!("{value} is not a value the parent ever sends"));
            assert_eq!(
                stop_bits_value(round_tripped),
                value,
                "stop bits {value} did not survive the round trip"
            );
        }
    }

    #[test]
    fn discriminant_values_are_rejected() {
        // 0..=3 are the discriminants of Five..Eight, never valid counts.
        for bad in 0..=3 {
            assert!(data_bits_from_value(bad).is_none(), "{bad} accepted");
        }
        assert!(data_bits_from_value(9).is_none());
        assert!(stop_bits_from_value(0).is_none());
        assert!(stop_bits_from_value(3).is_none());
    }

    /// Real values from this machine: COM6 is a PL2303, whose driver reports the
    /// manufacturer as just "Prolific" — useless for telling two boards apart.
    #[test]
    fn the_friendly_name_wins_over_the_manufacturer() {
        assert_eq!(
            port_label(
                Some("Prolific PL2303GT USB Serial COM Port (COM6)"),
                Some("Prolific")
            ),
            Some("Prolific PL2303GT USB Serial COM Port (COM6)".to_string())
        );
    }

    #[test]
    fn the_manufacturer_is_only_a_fallback() {
        assert_eq!(
            port_label(None, Some("Prolific")),
            Some("Prolific".to_string())
        );
        assert_eq!(port_label(None, None), None);
        assert_eq!(port_label(Some("USB Serial Device (COM4)"), None),
                   Some("USB Serial Device (COM4)".to_string()));
    }
}

