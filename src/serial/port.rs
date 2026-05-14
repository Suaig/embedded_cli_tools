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
    let result: Vec<PortInfo> = ports
        .into_iter()
        .map(|p| {
            let (manufacturer, vid, pid, serial_number) = match &p.port_type {
                serialport::SerialPortType::UsbPort(info) => (
                    info.manufacturer.clone(),
                    Some(info.vid),
                    Some(info.pid),
                    info.serial_number.clone(),
                ),
                _ => (None, None, None, None),
            };
            PortInfo {
                name: p.port_name,
                manufacturer,
                vid,
                pid,
                serial_number,
            }
        })
        .collect();
    Ok(result)
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

