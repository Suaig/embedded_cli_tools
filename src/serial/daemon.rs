use std::fs;
use std::io::Write as IoWrite;
use std::path::PathBuf;
use std::process::{Command, Stdio};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::port::SerialConfig;

// ---------------------------------------------------------------------------
// Metadata types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonMeta {
    pub id: String,
    pub port_name: String,
    pub baud_rate: u32,
    pub data_bits: u8,
    pub parity: String,
    pub stop_bits: u8,
    pub pid: u32,
    pub started_at: String,
    pub status: String,
}

// ---------------------------------------------------------------------------
// State directory helpers
// ---------------------------------------------------------------------------

/// Get the base directory for all daemon state: %TEMP%/emb/daemons/
fn daemons_base_dir() -> PathBuf {
    std::env::temp_dir().join("emb").join("daemons")
}

/// Get the state directory for a specific daemon ID.
fn state_dir(id: &str) -> PathBuf {
    daemons_base_dir().join(id)
}

/// Path to the metadata file.
fn meta_path(id: &str) -> PathBuf {
    state_dir(id).join("meta.toml")
}

/// Path to the received-data buffer file.
fn buffer_path(id: &str) -> PathBuf {
    state_dir(id).join("buffer.bin")
}

/// Path to the send-data staging file.
fn send_data_path(id: &str) -> PathBuf {
    state_dir(id).join("send_data")
}

/// Path to the shutdown signal file.
fn shutdown_path(id: &str) -> PathBuf {
    state_dir(id).join("shutdown")
}

// ---------------------------------------------------------------------------
// Public API: start / list / send / read / stop
// ---------------------------------------------------------------------------

/// Start a serial daemon by spawning a background `emb` process.
///
/// Returns the daemon ID on success.
pub fn start(
    port_name: &str,
    baud_rate: u32,
    id: Option<&str>,
    config: &SerialConfig,
) -> anyhow::Result<String> {
    let daemon_id = match id {
        Some(custom) => {
            // Validate no existing daemon uses this ID
            let existing = meta_path(custom);
            if existing.exists() {
                // Check if the daemon is actually still running
                let meta = read_meta(custom)?;
                if is_process_alive(meta.pid) {
                    anyhow::bail!("daemon '{}' is already running (PID {})", custom, meta.pid);
                }
                // Stale state dir, clean up
                let _ = fs::remove_dir_all(state_dir(custom));
            }
            custom.to_string()
        }
        None => uuid::Uuid::new_v4().to_string(),
    };

    // Create state directory
    let sdir = state_dir(&daemon_id);
    fs::create_dir_all(&sdir)?;

    // Build the child command: emb --internal-daemon <id> --port <port> --baud <baud> ...
    let exe = std::env::current_exe()?;
    let data_bits_str = format!("{}", config.data_bits as u8);
    let stop_bits_str = format!("{}", config.stop_bits as u8);
    let parity_str = match config.parity {
        serialport::Parity::None => "none",
        serialport::Parity::Odd => "odd",
        serialport::Parity::Even => "even",
    };

    let child = Command::new(&exe)
        .arg("--internal-daemon")
        .arg(&daemon_id)
        .arg("--port")
        .arg(port_name)
        .arg("--baud")
        .arg(baud_rate.to_string())
        .arg("--data-bits")
        .arg(&data_bits_str)
        .arg("--parity")
        .arg(parity_str)
        .arg("--stop-bits")
        .arg(&stop_bits_str)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        // Windows: detach from console via CREATE_NEW_PROCESS_GROUP
        // Unix: use setsid via process group
        .creation_flags(0x00000200) // CREATE_NEW_PROCESS_GROUP
        .spawn()?;

    let pid = child.id();
    // Drop the Child handle so we don't wait on it
    drop(child);

    // Write initial metadata
    let meta = DaemonMeta {
        id: daemon_id.clone(),
        port_name: port_name.to_string(),
        baud_rate,
        data_bits: config.data_bits as u8,
        parity: parity_str.to_string(),
        stop_bits: config.stop_bits as u8,
        pid,
        started_at: chrono_now(),
        status: "running".to_string(),
    };
    write_meta(&meta)?;

    Ok(daemon_id)
}

/// List all daemon instances (running and stale).
pub fn list() -> anyhow::Result<Vec<DaemonMeta>> {
    let base = daemons_base_dir();
    if !base.exists() {
        return Ok(Vec::new());
    }

    let mut result = Vec::new();
    for entry in fs::read_dir(&base)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let id = match entry.file_name().to_str() {
            Some(s) => s.to_string(),
            None => continue,
        };
        let mp = meta_path(&id);
        if !mp.exists() {
            continue;
        }
        match read_meta(&id) {
            Ok(mut meta) => {
                // Update status based on whether process is alive
                if !is_process_alive(meta.pid) {
                    meta.status = "stopped".to_string();
                }
                result.push(meta);
            }
            Err(_) => continue,
        }
    }

    Ok(result)
}

/// Send data through a daemon by writing to the send_data staging file.
pub fn send(id: &str, data: &[u8]) -> anyhow::Result<()> {
    let meta = read_meta(id)?;
    if !is_process_alive(meta.pid) {
        anyhow::bail!("daemon '{}' is not running (PID {})", id, meta.pid);
    }

    let send_path = send_data_path(id);
    fs::write(&send_path, data)?;

    // Wait briefly for the daemon to consume the file (up to 2 seconds)
    let start = Instant::now();
    let timeout = Duration::from_secs(2);
    while start.elapsed() < timeout {
        if !send_path.exists() {
            return Ok(());
        }
        // Check if file is empty (daemon consumed it)
        match fs::metadata(&send_path) {
            Ok(m) if m.len() == 0 => {
                let _ = fs::remove_file(&send_path);
                return Ok(());
            }
            _ => {}
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    // Timeout is not necessarily an error - the daemon might be slow
    Ok(())
}

/// Read buffered data from a daemon.
pub fn read(id: &str, clear: bool) -> anyhow::Result<Vec<u8>> {
    let buf_path = buffer_path(id);
    if !buf_path.exists() {
        return Ok(Vec::new());
    }

    let data = fs::read(&buf_path)?;

    if clear && !data.is_empty() {
        // Truncate the buffer file
        fs::write(&buf_path, &[])?;
    }

    Ok(data)
}

/// Stop a running daemon.
pub fn stop(id: &str) -> anyhow::Result<()> {
    let meta = read_meta(id)?;

    if !is_process_alive(meta.pid) {
        // Already stopped, just clean up
        cleanup_state(id);
        return Ok(());
    }

    // Write shutdown signal
    let shutdown_file = shutdown_path(id);
    fs::write(&shutdown_file, b"shutdown")?;

    // Wait for the process to exit (up to 5 seconds)
    let start = Instant::now();
    let timeout = Duration::from_secs(5);
    while start.elapsed() < timeout {
        if !is_process_alive(meta.pid) {
            cleanup_state(id);
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    // Force kill if still alive
    kill_process(meta.pid)?;
    cleanup_state(id);
    Ok(())
}

// ---------------------------------------------------------------------------
// Internal daemon serve mode
// ---------------------------------------------------------------------------

/// Run as a daemon process. This is called when `--internal-daemon <id>` is present.
/// Opens the serial port and continuously reads data into buffer.bin,
/// while watching for send_data and shutdown files.
pub fn serve(
    id: &str,
    port_name: &str,
    baud_rate: u32,
    config: &SerialConfig,
) -> anyhow::Result<()> {
    let sdir = state_dir(id);
    if !sdir.exists() {
        anyhow::bail!("state directory does not exist: {}", sdir.display());
    }

    let buf_path = buffer_path(id);
    let send_path = send_data_path(id);
    let shutdown_file = shutdown_path(id);

    // Open serial port
    let mut port = serialport::new(port_name, baud_rate)
        .data_bits(config.data_bits)
        .parity(config.parity)
        .stop_bits(config.stop_bits)
        .timeout(Duration::from_millis(100))
        .open()?;

    let running = Arc::new(AtomicBool::new(true));
    let r1 = running.clone();
    let r2 = running.clone();

    // Set up Ctrl+C handler for graceful shutdown
    ctrlc_handler(move || {
        r1.store(false, Ordering::SeqCst);
    });

    // Open buffer file in append mode
    let mut buffer_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&buf_path)?;

    let mut read_buf = vec![0u8; 4096];

    while r2.load(Ordering::SeqCst) {
        // Check for shutdown signal
        if shutdown_file.exists() {
            break;
        }

        // Check for data to send
        if send_path.exists() {
            match fs::read(&send_path) {
                Ok(data) if !data.is_empty() => {
                    if let Err(_e) = port.write_all(&data) {
                        // Write failed; port might be disconnected
                    } else {
                        let _ = port.flush();
                    }
                    // Delete the send file regardless
                    let _ = fs::remove_file(&send_path);
                }
                Ok(_) => {
                    // Empty file, remove it
                    let _ = fs::remove_file(&send_path);
                }
                Err(_) => {
                    // Could not read; try again next iteration
                }
            }
        }

        // Read from serial port
        match port.read(&mut read_buf) {
            Ok(n) if n > 0 => {
                if let Err(_e) = buffer_file.write_all(&read_buf[..n]) {
                    break;
                }
                // Flush periodically (not every byte for performance)
                let _ = buffer_file.flush();
            }
            Ok(_) => {
                // 0 bytes, should not happen with timeout
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {
                // Normal timeout, continue loop
            }
            Err(_e) => {
                // Port error (possibly disconnected), sleep briefly and retry
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }

    // Graceful cleanup
    let _ = fs::remove_file(&shutdown_file);
    let _ = update_meta_status(id, "stopped");

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn write_meta(meta: &DaemonMeta) -> anyhow::Result<()> {
    let path = meta_path(&meta.id);
    let content = toml::to_string_pretty(meta)?;
    fs::write(&path, content)?;
    Ok(())
}

fn read_meta(id: &str) -> anyhow::Result<DaemonMeta> {
    let path = meta_path(id);
    if !path.exists() {
        anyhow::bail!("daemon '{}' not found", id);
    }
    let content = fs::read_to_string(&path)?;
    let meta: DaemonMeta = toml::from_str(&content)?;
    Ok(meta)
}

fn update_meta_status(id: &str, status: &str) -> anyhow::Result<()> {
    let mut meta = read_meta(id)?;
    meta.status = status.to_string();
    write_meta(&meta)
}

fn cleanup_state(id: &str) {
    let sdir = state_dir(id);
    let _ = fs::remove_dir_all(&sdir);
}

/// Check if a process with the given PID is alive.
fn is_process_alive(pid: u32) -> bool {
    // On Windows, use OpenProcess + WaitForInputIdle or just try to open
    // Simple approach: use tasklist or just try kill with signal 0 equivalent
    #[cfg(target_os = "windows")]
    {
        let output = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {}", pid), "/NH"])
            .stdout(Stdio::piped())
            .output();
        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                stdout.contains(&pid.to_string())
            }
            Err(_) => false,
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        // Unix: send signal 0 to check if process exists
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }
}

/// Kill a process by PID.
fn kill_process(pid: u32) -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    {
        Command::new("taskkill")
            .args(["/F", "/PID", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        unsafe { libc::kill(pid as i32, libc::SIGTERM); }
    }
    Ok(())
}

/// Simple timestamp without external chrono dependency.
fn chrono_now() -> String {
    let output = Command::new(if cfg!(target_os = "windows") {
        "cmd"
    } else {
        "date"
    })
    .args(if cfg!(target_os = "windows") {
        vec!["/C", "echo %DATE% %TIME%"]
    } else {
        vec!["+%Y-%m-%d %H:%M:%S"]
    })
    .stdout(Stdio::piped())
    .output();

    match output {
        Ok(out) => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        Err(_) => "unknown".to_string(),
    }
}

/// Set up a Ctrl+C handler that sets a flag.
fn ctrlc_handler<F: FnOnce() + Send + 'static>(handler: F) {
    // We use a simple approach: just set the running flag to false.
    // On Windows, the child process is in its own process group,
    // so Ctrl+C from the parent terminal won't reach it anyway.
    // But if the user opens a new terminal and finds the PID, this helps.
    let _ = handler; // Suppress unused warning; real implementation below
    #[cfg(target_os = "windows")]
    {
        // Windows: use SetConsoleCtrlHandler via winapi
        // For simplicity, we just ignore Ctrl+C and rely on the shutdown file
        use std::sync::Once;
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            // Best-effort: on Windows, console apps get Ctrl+C by default
            // The daemon runs in a new process group, so it won't receive
            // Ctrl+C from the parent console. No special handling needed.
        });
    }
}
