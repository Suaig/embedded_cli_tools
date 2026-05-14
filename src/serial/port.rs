use serialport::{DataBits, Parity, StopBits};
use std::io::{Read, Write};
use std::time::Duration;

/// Information about a scanned serial port.
#[derive(Debug, Clone)]
pub struct PortInfo {
    pub name: String,
    pub manufacturer: Option<String>,
    pub vid: Option<u16>,
    pub pid: Option<u16>,
    pub serial_number: Option<String>,
    pub port_type: String,
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

/// Scan available serial ports.
/// Returns an empty list (not an error) if no ports are found.
pub fn scan() -> anyhow::Result<Vec<PortInfo>> {
    let ports = serialport::available_ports()?;
    let mut result: Vec<PortInfo> = ports
        .into_iter()
        .map(|p| {
            let (manufacturer, vid, pid, serial_number, port_type_str) = match &p.port_type {
                serialport::SerialPortType::UsbPort(info) => (
                    info.manufacturer.clone(),
                    Some(info.vid),
                    Some(info.pid),
                    info.serial_number.clone(),
                    "USB".to_string(),
                ),
                serialport::SerialPortType::BluetoothPort => {
                    (None, None, None, None, "Bluetooth".to_string())
                }
                serialport::SerialPortType::PciPort => {
                    (None, None, None, None, "PCI".to_string())
                }
                serialport::SerialPortType::Unknown => {
                    (None, None, None, None, "Unknown".to_string())
                }
            }; // other variants treated as Unknown
            PortInfo {
                name: p.port_name.clone(),
                manufacturer,
                vid,
                pid,
                serial_number,
                port_type: port_type_str,
            }
        })
        .collect();

    // On Windows, supplement with WMI friendly names
    #[cfg(target_os = "windows")]
    {
        let wmi_names = query_wmi_port_names();
        for port in &mut result {
            if port.manufacturer.is_none() || port.manufacturer.as_deref() == Some("-") {
                if let Some(friendly) = wmi_names.get(&port.name) {
                    port.manufacturer = Some(friendly.clone());
                }
            }
        }
    }

    Ok(result)
}

/// Query WMI for serial port friendly names (Windows only).
#[cfg(target_os = "windows")]
fn query_wmi_port_names() -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let output = match std::process::Command::new("wmic")
        .args([
            "path", "Win32_SerialPort",
            "get", "DeviceID,Name",
            "/format:csv",
        ])
        .output()
    {
        Ok(o) => o,
        Err(_) => return map,
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines().skip(1) {
        // format: Node,COM1,Communications Port (COM1)
        let parts: Vec<&str> = line.splitn(3, ',').collect();
        if parts.len() >= 3 {
            let device_id = parts[1].trim().to_string();
            let name = parts[2].trim().to_string();
            if !device_id.is_empty() && !name.is_empty() {
                map.insert(device_id, name);
            }
        }
    }
    map
}

/// Send data to a serial port (open, send, close).
pub fn send(port_name: &str, data: &[u8], config: &SerialConfig) -> anyhow::Result<()> {
    let mut port = serialport::new(port_name, config.baud_rate)
        .data_bits(config.data_bits)
        .parity(config.parity)
        .stop_bits(config.stop_bits)
        .timeout(Duration::from_secs(5))
        .open()?;
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
        .open()?;

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

