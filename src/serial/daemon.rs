use std::fs;
use std::io::Write as IoWrite;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::port::SerialConfig;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Max history file size before pruning (5 MB).
const HISTORY_MAX_SIZE: u64 = 5 * 1024 * 1024;

/// Cap on unread received bytes.
///
/// Nothing else prunes buffer.bin - it only shrinks when `read` collects it. A
/// board that talks continuously while nobody reads (a `cat /dev/zero`, a log
/// spew) would otherwise grow it until the disk fills, and `read` would then
/// load the whole thing into memory.
const BUFFER_MAX_SIZE: u64 = 4 * 1024 * 1024;

/// How long `start` waits for the detached child to report that it came up.
/// Only ever paid in full when the child is slow to open the port; a healthy
/// start returns as soon as the child writes its `ready` file.
const START_SETTLE_MS: u64 = 2000;

/// When history exceeds the limit, prune to this ratio (keep last 50%).
const HISTORY_KEEP_RATIO: f64 = 0.5;

/// Max entries displayed by default for `history` command.
const HISTORY_DEFAULT_LIMIT: usize = 100;

/// Max entries displayable (hard cap).
const HISTORY_MAX_LIMIT: usize = 1000;

/// Max hex data length stored per history entry (bytes).
const HISTORY_ENTRY_MAX_DATA: usize = 256;

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

/// A single history entry for send/receive events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// ISO 8601 timestamp.
    pub ts: String,
    /// "recv" or "send".
    pub dir: String,
    /// Total data length in bytes.
    pub len: usize,
    /// Hex-encoded data (may be truncated for large payloads).
    pub hex: String,
    /// Whether the hex data was truncated.
    pub truncated: bool,
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

/// Written by the daemon once startup succeeded - the parent waits for it.
fn ready_path(id: &str) -> PathBuf {
    state_dir(id).join("ready")
}

/// Written by the daemon instead of `ready` when startup failed, holding the
/// reason. The child's stderr is NUL, so a bare error return would be invisible
/// to the parent that spawned it.
fn error_path(id: &str) -> PathBuf {
    state_dir(id).join("error.txt")
}

/// Written by the daemon when it could not push staged bytes onto the wire
/// (port unplugged, driver wedged). Without it a failed send looks exactly
/// like a successful one: the staging file disappears either way.
fn send_error_path(id: &str) -> PathBuf {
    state_dir(id).join("send_error.txt")
}

/// Path to the shutdown signal file.
fn shutdown_path(id: &str) -> PathBuf {
    state_dir(id).join("shutdown")
}

/// Path to the send/receive history file (JSONL format).
fn history_path(id: &str) -> PathBuf {
    state_dir(id).join("history.jsonl")
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
            validate_id(custom)?;
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
    // A reused id may carry a previous run's verdict; start from a clean slate.
    let _ = fs::remove_file(ready_path(&daemon_id));
    let _ = fs::remove_file(error_path(&daemon_id));

    // Build the child command: emb --internal-daemon <id> --port <port> --baud <baud> ...
    let exe = std::env::current_exe()?;
    let data_bits_str = crate::serial::port::data_bits_value(config.data_bits).to_string();
    let stop_bits_str = crate::serial::port::stop_bits_value(config.stop_bits).to_string();
    let parity_str = match config.parity {
        serialport::Parity::None => "none",
        serialport::Parity::Odd => "odd",
        serialport::Parity::Even => "even",
    };

    // spawn_detached, not Command::spawn: std's Windows Command cannot restrict
    // handle inheritance, so the detached daemon would inherit and keep holding
    // whatever pipe our caller attached to us. See src/serial/spawn.rs.
    let pid = super::spawn::spawn_detached(
        &exe,
        &[
            "--internal-daemon".to_string(),
            daemon_id.clone(),
            "--port".to_string(),
            port_name.to_string(),
            "--baud".to_string(),
            baud_rate.to_string(),
            "--data-bits".to_string(),
            data_bits_str,
            "--parity".to_string(),
            parity_str.to_string(),
            "--stop-bits".to_string(),
            stop_bits_str,
        ],
    )?;

    // Write initial metadata
    let meta = DaemonMeta {
        id: daemon_id.clone(),
        port_name: port_name.to_string(),
        baud_rate,
        data_bits: crate::serial::port::data_bits_value(config.data_bits),
        parity: parity_str.to_string(),
        stop_bits: crate::serial::port::stop_bits_value(config.stop_bits),
        pid,
        started_at: chrono_now(),
        status: "running".to_string(),
    };
    write_meta(&meta)?;

    // The child is detached, so nothing otherwise tells us whether it actually
    // came up: a busy port or a bad port name kills it within milliseconds and
    // we would report success regardless. Wait for it to either confirm (it
    // writes `ready`) or explain (it writes `error.txt`).
    let deadline = Instant::now() + Duration::from_millis(START_SETTLE_MS);
    loop {
        if error_path(&daemon_id).exists() {
            let reason = fs::read_to_string(error_path(&daemon_id)).unwrap_or_default();
            cleanup_state(&daemon_id);
            anyhow::bail!(
                "daemon '{}' failed to start: {}",
                daemon_id,
                reason.trim()
            );
        }
        if ready_path(&daemon_id).exists() {
            break;
        }
        if Instant::now() >= deadline {
            // Neither signal arrived. One last look before deciding: a process
            // that is gone did not make it, one that is alive is just slow
            // (some USB-serial drivers take a while to open).
            if !is_process_alive(pid) {
                cleanup_state(&daemon_id);
                anyhow::bail!(
                    "daemon '{}' exited during startup (PID {}) - no reason recorded",
                    daemon_id,
                    pid
                );
            }
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }

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
    let err_path = send_error_path(id);
    let _ = fs::remove_file(&err_path); // a previous failure is not this one
    fs::write(&send_path, data)?;

    // The daemon picks the file up within one loop iteration (its port read
    // times out after 100 ms), so approaching the deadline below means it is
    // not doing its job, not that it is a bit slow.
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let consumed = match fs::metadata(&send_path) {
            Ok(m) => m.len() == 0,
            Err(_) => true, // gone: consumed
        };
        if consumed {
            let _ = fs::remove_file(&send_path);
            break;
        }
        if Instant::now() >= deadline {
            let _ = fs::remove_file(&send_path);
            anyhow::bail!(
                "daemon '{}' did not pick up the data within 2s - is it wedged? \
                 (`emb serial daemon list` shows its state)",
                id
            );
        }
        std::thread::sleep(Duration::from_millis(25));
    }

    // Taking the file is not the same as putting the bytes on the wire.
    if let Ok(reason) = fs::read_to_string(&err_path) {
        let _ = fs::remove_file(&err_path);
        anyhow::bail!(
            "daemon '{}' could not write to the port: {}",
            id,
            reason.trim()
        );
    }

    Ok(())
}

/// Read unread data from a daemon. After reading, the buffer is cleared
/// so subsequent reads only return new data.
pub fn read(id: &str) -> anyhow::Result<Vec<u8>> {
    // A missing state directory means the id is wrong, not that there is no data.
    read_meta(id)?;

    let buf_path = buffer_path(id);
    if !buf_path.exists() {
        return Ok(Vec::new());
    }

    let data = fs::read(&buf_path)?;

    // Clear the buffer so next read only gets new data
    if !data.is_empty() {
        fs::write(&buf_path, [])?;
    }

    Ok(data)
}

/// List history entries for a daemon.
pub fn history(id: &str, limit: Option<usize>) -> anyhow::Result<Vec<HistoryEntry>> {
    // A missing state directory means the id is wrong, not that there is no history.
    read_meta(id)?;

    let hist_path = history_path(id);
    if !hist_path.exists() {
        return Ok(Vec::new());
    }

    let max = limit.unwrap_or(HISTORY_DEFAULT_LIMIT).min(HISTORY_MAX_LIMIT);
    let content = fs::read_to_string(&hist_path)?;

    let entries: Vec<HistoryEntry> = content
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();

    // Return the last `max` entries
    let start = entries.len().saturating_sub(max);
    Ok(entries[start..].to_vec())
}

/// Clear the history file for a daemon.
pub fn history_clear(id: &str) -> anyhow::Result<()> {
    // A missing state directory means the id is wrong, not that there is no history.
    read_meta(id)?;

    let hist_path = history_path(id);
    if hist_path.exists() {
        fs::write(&hist_path, [])?;
    }
    Ok(())
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
    let hist_path = history_path(id);

    // Open serial port. This is the failure users actually hit (port busy from
    // another daemon or another program, port gone), and this process's stderr
    // is NUL - so record the reason where the parent can pick it up, instead of
    // returning an error nobody ever sees.
    let mut port = match serialport::new(port_name, baud_rate)
        .data_bits(config.data_bits)
        .parity(config.parity)
        .stop_bits(config.stop_bits)
        .timeout(Duration::from_millis(100))
        .open()
    {
        Ok(p) => p,
        Err(e) => {
            record_start_error(id, &crate::serial::port::describe_open_error(port_name, &e));
            return Err(e.into());
        }
    };

    let running = Arc::new(AtomicBool::new(true));
    let r1 = running.clone();
    let r2 = running.clone();

    // Set up Ctrl+C handler for graceful shutdown
    ctrlc_handler(move || {
        r1.store(false, Ordering::SeqCst);
    });

    // Open buffer file in append mode
    let mut buffer_file = match fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&buf_path)
    {
        Ok(f) => f,
        Err(e) => {
            record_start_error(id, &format!("cannot open {}: {e}", buf_path.display()));
            return Err(e.into());
        }
    };

    // Startup is done. The parent is waiting on this to tell "came up" apart
    // from "died in the first millisecond".
    let _ = fs::write(ready_path(id), b"\n");

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
                    if let Err(e) = port.write_all(&data) {
                        // The sender is waiting to hear that the bytes went
                        // out, not merely that we took the file. Say so.
                        let _ = fs::write(send_error_path(id), format!("{e}\n"));
                    } else {
                        let _ = port.flush();
                        append_history(&hist_path, "send", &data);
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
                let _ = buffer_file.flush();
                append_history(&hist_path, "recv", &read_buf[..n]);
                // Prune history if it exceeds the size limit
                let _ = prune_history_if_needed(&hist_path);
                let _ = trim_buffer_if_needed(&buf_path);
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
    validate_id(id)?;
    let path = meta_path(id);
    if !path.exists() {
        anyhow::bail!("daemon '{}' not found", id);
    }
    let content = fs::read_to_string(&path)?;
    let meta: DaemonMeta = toml::from_str(&content)?;
    Ok(meta)
}

/// An id names a directory under the daemons base dir, so it must be a plain
/// name. Without this, `--id ../somewhere` writes outside that dir - the id
/// reaches `state_dir()` unchecked.
fn validate_id(id: &str) -> anyhow::Result<()> {
    const RESERVED: [char; 9] = ['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
    if id.is_empty() {
        anyhow::bail!("daemon id must not be empty");
    }
    if id == "." || id == ".." || id.chars().any(|c| RESERVED.contains(&c) || c.is_control()) {
        anyhow::bail!(
            "invalid daemon id '{id}': an id is a plain name - no path separators, \
             drive letters or reserved characters"
        );
    }
    Ok(())
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

/// Record why a daemon could not come up, for the parent that spawned it.
fn record_start_error(id: &str, msg: &str) {
    let _ = fs::write(error_path(id), format!("{msg}\n"));
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

// ---------------------------------------------------------------------------
// History helpers
// ---------------------------------------------------------------------------

/// Append a history entry to the JSONL file.
fn append_history(path: &PathBuf, dir: &str, data: &[u8]) {
    let truncated = data.len() > HISTORY_ENTRY_MAX_DATA;
    let hex_data = if truncated {
        super::protocol::encode_hex(&data[..HISTORY_ENTRY_MAX_DATA])
    } else {
        super::protocol::encode_hex(data)
    };

    let entry = HistoryEntry {
        ts: chrono_now(),
        dir: dir.to_string(),
        len: data.len(),
        hex: hex_data,
        truncated,
    };

    if let Ok(line) = serde_json::to_string(&entry) {
        // Open in append mode
        if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(f, "{}", line);
        }
    }
}

/// Prune history file if it exceeds the size limit.
/// Keeps the last ~50% of entries.
fn prune_history_if_needed(path: &PathBuf) -> anyhow::Result<()> {
    let meta = fs::metadata(path)?;
    if meta.len() <= HISTORY_MAX_SIZE {
        return Ok(());
    }

    let content = fs::read_to_string(path)?;
    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();

    let keep_count = ((lines.len() as f64) * HISTORY_KEEP_RATIO) as usize;
    let start = lines.len().saturating_sub(keep_count);
    let kept: String = lines[start..].join("\n");

    fs::write(path, kept)?;
    Ok(())
}

/// Drop the OLDEST bytes once buffer.bin passes BUFFER_MAX_SIZE.
///
/// The file holds data `read` has not collected yet, so when it overflows the
/// newest bytes are the ones still worth keeping. It trims down to half the cap
/// rather than to the cap, so a continuous stream does not pay a 4 MB rewrite
/// on every chunk that follows.
fn trim_buffer_if_needed(path: &PathBuf) -> anyhow::Result<()> {
    let meta = fs::metadata(path)?;
    if meta.len() <= BUFFER_MAX_SIZE {
        return Ok(());
    }

    let data = fs::read(path)?;
    let from = data.len().saturating_sub((BUFFER_MAX_SIZE / 2) as usize);
    fs::write(path, &data[from..])?;
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Bytes that make a wrong slice obvious: every position is distinguishable.
    fn numbered(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn trim_buffer_leaves_a_small_buffer_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("buffer.bin");
        fs::write(&path, numbered(1024)).unwrap();

        trim_buffer_if_needed(&path).unwrap();

        assert_eq!(fs::metadata(&path).unwrap().len(), 1024);
    }

    #[test]
    fn trim_buffer_keeps_the_newest_half_of_an_overflowing_buffer() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("buffer.bin");
        let total = BUFFER_MAX_SIZE as usize + 4096;
        let data = numbered(total);
        fs::write(&path, &data).unwrap();

        trim_buffer_if_needed(&path).unwrap();

        let kept = fs::read(&path).unwrap();
        let expected = (BUFFER_MAX_SIZE / 2) as usize;
        assert_eq!(kept.len(), expected);
        assert_eq!(kept, &data[total - expected..], "must keep the newest bytes");
    }
}
