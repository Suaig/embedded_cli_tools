use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use anyhow::Context;
use serde::Serialize;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Keil build sub-command passed to UV4.exe.
#[derive(Debug, Clone, Copy)]
pub enum BuildCommand {
    Build,
    Rebuild,
    Clean,
    Flash,
}

impl BuildCommand {
    fn uv4_flag(self) -> &'static str {
        match self {
            BuildCommand::Build => "-b",
            BuildCommand::Rebuild => "-r",
            BuildCommand::Clean => "-c",
            BuildCommand::Flash => "-f",
        }
    }

}

/// How long UV4 may consume no CPU at all before it is called stuck.
///
/// `-j0` hides the window but does not stop μVision from raising a modal
/// dialog, and a dialog blocks the process at zero CPU. That is exactly the
/// state that reads like "a big build, be patient" for minutes on end — the
/// build that motivated this sat frozen for over two minutes with a 0-byte log.
/// A build that is actually working always burns CPU, so this cannot fire on a
/// slow-but-healthy one. A licence checkout wait looks the same from outside;
/// it is just as hopeless to wait on from a non-interactive caller.
const STALL_LIMIT_SECS: u64 = 30;

/// How often the child and its log are looked at.
const POLL_INTERVAL_MS: u64 = 100;

/// Supervision options for a build, as opposed to the build's content.
#[derive(Debug, Clone, Copy, Default)]
pub struct BuildOptions {
    /// Wall-clock cap in seconds. 0 means no cap: a legitimate full rebuild of
    /// a large project can run for many minutes, and killing it would be worse
    /// than waiting. The stall detector is the guard that actually matters.
    pub timeout_secs: u64,
    /// Echo the build log to stderr as it grows.
    ///
    /// Opt-in: a real rebuild produces tens of kilobytes of log, which is more
    /// noise than an agent wants on a build that passes. Turn it on when a
    /// build is slow or stuck and you need to see where it got to.
    pub stream: bool,
}

/// Parsed program size from the build log.
#[derive(Debug, Clone, Serialize)]
pub struct ProgramSize {
    pub code: u64,
    pub ro_data: u64,
    pub rw_data: u64,
    pub zi_data: u64,
}

/// Result of a build operation.
#[derive(Debug, Clone, Serialize)]
pub struct BuildResult {
    pub success: bool,
    pub errors: u32,
    pub warnings: u32,
    pub build_time: String,
    pub program_size: Option<ProgramSize>,
    pub output_file: String,
    pub log: String,
}

// ---------------------------------------------------------------------------
// UV4.exe discovery
// ---------------------------------------------------------------------------

/// Locate UV4.exe (or UV4.com) on the system.
///
/// Search order:
/// 1. `UV4_PATH` environment variable
/// 2. `KEIL_PATH` environment variable (expects root like `C:\Keil_v5`)
/// 3. The Keil registry entry (covers per-user installs under
///    `%LOCALAPPDATA%\Keil_v5`, which no fixed path reaches)
/// 4. Common fixed paths
/// 5. `%LOCALAPPDATA%` candidates
/// 6. Scan `C:\Program Files` and `C:\Program Files (x86)` for Keil directories
pub fn find_uv4() -> anyhow::Result<PathBuf> {
    // 1. Explicit UV4_PATH
    if let Ok(p) = std::env::var("UV4_PATH") {
        let path = PathBuf::from(&p);
        if path.is_file() {
            return Ok(path);
        }
    }

    // 2. KEIL_PATH (root directory)
    if let Ok(root) = std::env::var("KEIL_PATH") {
        let uv4 = PathBuf::from(&root).join("UV4").join("UV4.exe");
        if uv4.is_file() {
            return Ok(uv4);
        }
    }

    // 3. Registry
    if let Some(uv4) = uv4_from_registry() {
        return Ok(uv4);
    }

    // 4. Common fixed paths
    let common = [
        r"C:\Keil_v5\UV4\UV4.exe",
        r"C:\Keil\UV4\UV4.exe",
    ];
    for p in &common {
        let path = PathBuf::from(p);
        if path.is_file() {
            return Ok(path);
        }
    }

    // 5. %LOCALAPPDATA% — where the per-user MDK installer puts Keil_v5
    let mut local_appdata_candidates: Vec<PathBuf> = Vec::new();
    if let Some(base) = std::env::var_os("LOCALAPPDATA") {
        let base = PathBuf::from(base);
        local_appdata_candidates.push(base.join("Keil_v5").join("UV4").join("UV4.exe"));
        local_appdata_candidates.push(base.join("Keil").join("UV4").join("UV4.exe"));
    }
    for path in &local_appdata_candidates {
        if path.is_file() {
            return Ok(path.clone());
        }
    }

    // 4. Search Program Files directories
    let program_dirs = [
        r"C:\Program Files",
        r"C:\Program Files (x86)",
    ];
    for dir in &program_dirs {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_lower = name.to_string_lossy().to_lowercase();
                if name_lower.starts_with("keil") {
                    // Try UV4\UV4.exe under the Keil root
                    let uv4 = entry.path().join("UV4").join("UV4.exe");
                    if uv4.is_file() {
                        return Ok(uv4);
                    }
                    // Some installs nest under ARM or MDK
                    for sub in &["ARM", "MDK"] {
                        let nested = entry.path().join(sub).join("UV4").join("UV4.exe");
                        if nested.is_file() {
                            return Ok(nested);
                        }
                    }
                }
            }
        }
    }

    anyhow::bail!(
        "UV4.exe not found. \
         Set UV4_PATH to the full path of UV4.exe, \
         or set KEIL_PATH to the Keil installation root (e.g. C:\\Keil_v5)"
    )
}

/// Read the Keil MDK install root out of the registry and return UV4.exe.
///
/// Recent MDK installers default to a per-user location under
/// `%LOCALAPPDATA%\Keil_v5`, which none of the fixed paths reach, but the
/// installer always records it here. The value names the ARM toolchain
/// directory (`...\Keil_v5\ARM`), so UV4 is its sibling.
#[cfg(target_os = "windows")]
fn uv4_from_registry() -> Option<PathBuf> {
    const KEYS: [&str; 2] = [
        r"HKLM\SOFTWARE\WOW6432Node\Keil\Products\MDK",
        r"HKLM\SOFTWARE\Keil\Products\MDK",
    ];

    for key in KEYS {
        let Ok(output) = Command::new("reg")
            .args(["query", key, "/v", "Path"])
            .output()
        else {
            continue;
        };
        if !output.status.success() {
            continue;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let Some(arm_dir) = parse_reg_path(&stdout) else {
            continue;
        };
        if let Some(uv4) = uv4_beside_arm_dir(Path::new(&arm_dir)) {
            if uv4.is_file() {
                return Some(uv4);
            }
        }
    }
    None
}

#[cfg(not(target_os = "windows"))]
fn uv4_from_registry() -> Option<PathBuf> {
    None
}

/// Pull the `REG_SZ` value out of `reg query` output:
///
/// ```text
/// HKEY_LOCAL_MACHINE\SOFTWARE\WOW6432Node\Keil\Products\MDK
///     Path    REG_SZ    C:\Users\me\AppData\Local\Keil_v5\ARM
/// ```
fn parse_reg_path(stdout: &str) -> Option<String> {
    for line in stdout.lines() {
        let line = line.trim();
        if !line.starts_with("Path") {
            continue;
        }
        if let Some(idx) = line.find("REG_SZ") {
            let value = line[idx + "REG_SZ".len()..].trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// `...\Keil_v5\ARM` -> `...\Keil_v5\UV4\UV4.exe`.
fn uv4_beside_arm_dir(arm_dir: &Path) -> Option<PathBuf> {
    let root = arm_dir.parent()?;
    if root.as_os_str().is_empty() {
        return None;
    }
    Some(root.join("UV4").join("UV4.exe"))
}

/// Prefer UV4.com (console output) when available; fall back to UV4.exe.
fn prefer_com(uv4: &Path) -> anyhow::Result<PathBuf> {
    if let Some(stem) = uv4.file_stem() {
        let com_name = format!("{}.com", stem.to_string_lossy());
        if let Some(parent) = uv4.parent() {
            let com_path = parent.join(&com_name);
            if com_path.is_file() {
                return Ok(com_path);
            }
        }
    }
    Ok(uv4.to_path_buf())
}

fn resolve_uv4_binary() -> anyhow::Result<PathBuf> {
    let uv4 = find_uv4()?;
    prefer_com(&uv4)
}

// ---------------------------------------------------------------------------
// Build log parsing
// ---------------------------------------------------------------------------

/// Parse the build log produced by UV4.exe.
fn parse_build_log(log: &str) -> (u32, u32, String, Option<ProgramSize>, String) {
    let mut errors: u32 = 0;
    let mut warnings: u32 = 0;
    let mut build_time = String::new();
    let mut program_size = None;
    let mut output_file = String::new();

    for line in log.lines() {
        let trimmed = line.trim();

        // Match: ".\Objects\H743_PRO.axf" - 0 Error(s), 0 Warning(s).
        if let Some(rest) = trimmed.strip_prefix('"') {
            if let Some(quote_end) = rest.find('"') {
                let file_path = &rest[..quote_end];
                let after = rest[quote_end + 1..].trim();
                if after.starts_with('-') {
                    output_file = file_path.trim_start_matches('.').trim_start_matches('\\').to_string();
                    // Parse error/warning counts
                    let remainder = after.trim_start_matches('-').trim();
                    parse_error_warning_counts(remainder, &mut errors, &mut warnings);
                }
            }
        }

        // Match: Program Size: Code=12345 RO-data=6789 RW-data=123 ZI-data=4567
        if let Some(rest) = trimmed.strip_prefix("Program Size:") {
            program_size = parse_program_size(rest.trim());
        }

        // Match: Build Time Elapsed:  00:00:05
        if let Some(rest) = trimmed.strip_prefix("Build Time Elapsed:") {
            build_time = rest.trim().to_string();
        }
    }

    (errors, warnings, build_time, program_size, output_file)
}

fn parse_error_warning_counts(s: &str, errors: &mut u32, warnings: &mut u32) {
    // Format: "0 Error(s), 0 Warning(s)."
    let parts: Vec<&str> = s.split(',').collect();
    for part in parts {
        let part = part.trim().trim_end_matches('.');
        if let Some(num_str) = part.strip_suffix("Error(s)") {
            *errors = num_str.trim().parse().unwrap_or(0);
        } else if let Some(num_str) = part.strip_suffix("Warning(s)") {
            *warnings = num_str.trim().parse().unwrap_or(0);
        }
    }
}

fn parse_program_size(s: &str) -> Option<ProgramSize> {
    let mut code = None;
    let mut ro_data = None;
    let mut rw_data = None;
    let mut zi_data = None;

    for token in s.split_whitespace() {
        if let Some((key, val)) = token.split_once('=') {
            if let Ok(v) = val.parse::<u64>() {
                match key {
                    "Code" => code = Some(v),
                    "RO-data" => ro_data = Some(v),
                    "RW-data" => rw_data = Some(v),
                    "ZI-data" => zi_data = Some(v),
                    _ => {}
                }
            }
        }
    }

    Some(ProgramSize {
        code: code?,
        ro_data: ro_data?,
        rw_data: rw_data?,
        zi_data: zi_data?,
    })
}

// ---------------------------------------------------------------------------
// Build execution
// ---------------------------------------------------------------------------

/// Run a Keil build command via UV4.exe.
///
/// `path` is the .uvprojx project file path.
/// `target` optionally selects the build target.
/// `command` selects build / rebuild / clean / flash.
/// `opts` governs supervision (timeout cap, log streaming), not the build.
pub fn build(
    path: &Path,
    target: &Option<String>,
    command: BuildCommand,
    uv4_path: Option<PathBuf>,
    opts: &BuildOptions,
) -> anyhow::Result<BuildResult> {
    let uv4 = match uv4_path {
        Some(p) => prefer_com(&p).context("failed to locate UV4.exe")?,
        None => resolve_uv4_binary().context("failed to locate UV4.exe")?,
    };

    if !path.is_file() {
        anyhow::bail!("project file not found: {}", path.display());
    }

    // Clean does not produce a meaningful log; run and check exit code only.
    if matches!(command, BuildCommand::Clean) {
        return run_clean(&uv4, path, target, opts);
    }

    // Create a temp file for the build log.
    let temp_dir = std::env::temp_dir();
    let log_file = temp_dir.join(format!("emb_build_{}.log", uuid::Uuid::new_v4().simple()));

    let run = run_uv4(&uv4, path, target, command, &log_file, opts);

    // Read the log regardless of exit status.
    let log_content = std::fs::read_to_string(&log_file).unwrap_or_default();

    // Clean up temp log file (best-effort).
    let _ = std::fs::remove_file(&log_file);

    // A timed-out or stuck UV4 never reports anything itself; whatever it
    // managed to write is the only clue about where it stopped.
    let exit_code = match run {
        Ok(code) => code,
        Err(err) => {
            let tail = tail_lines(&log_content, 15);
            if tail.is_empty() {
                return Err(err);
            }
            anyhow::bail!("{err}\n--- build log, last 15 lines ---\n{tail}")
        }
    };

    let (errors, warnings, build_time, program_size, output_file) =
        parse_build_log(&log_content);

    let success = errors == 0 && exit_code <= 1;

    Ok(BuildResult {
        success,
        errors,
        warnings,
        build_time,
        program_size,
        output_file,
        log: log_content,
    })
}

fn run_uv4(
    uv4: &Path,
    project: &Path,
    target: &Option<String>,
    command: BuildCommand,
    log_file: &Path,
    opts: &BuildOptions,
) -> anyhow::Result<u32> {
    let mut cmd = Command::new(uv4);

    // Suppress GUI (-j0 = hide uVision window; -sg is not a valid UV4 flag
    // and was making UV4 emit an empty log on some installs).
    cmd.arg("-j0");

    // Build command flag
    cmd.arg(command.uv4_flag());

    // Project file (absolute path works best)
    let abs_project = std::path::absolute(project)?;
    cmd.arg(&abs_project);

    // Optional target
    if let Some(t) = target {
        cmd.arg("-t").arg(t);
    }

    // Log output
    cmd.arg("-o").arg(log_file);

    // Nothing may read our stdin: if UV4 ever does decide to show a dialog, it
    // must not be handed whatever the caller attached to the terminal.
    cmd.stdin(Stdio::null());

    let mut child = cmd
        .spawn()
        .with_context(|| format!("failed to execute UV4 at {}", uv4.display()))?;

    let mut poller = LogPoller::new(log_file, opts.stream);
    let status = supervise(&mut child, Some(&mut poller), opts)?;

    // UV4 exit code semantics: 0=success, 1=warnings, 2+=errors, 20=special errors
    Ok(status.code().unwrap_or(1) as u32)
}

fn run_clean(
    uv4: &Path,
    project: &Path,
    target: &Option<String>,
    opts: &BuildOptions,
) -> anyhow::Result<BuildResult> {
    let mut cmd = Command::new(uv4);
    cmd.arg("-j0");
    cmd.arg("-c");

    let abs_project = std::path::absolute(project)?;
    cmd.arg(abs_project);

    if let Some(t) = target {
        cmd.arg("-t").arg(t);
    }

    cmd.stdin(Stdio::null());

    let mut child = cmd
        .spawn()
        .with_context(|| format!("failed to execute UV4 at {}", uv4.display()))?;
    let status = supervise(&mut child, None, opts)?;

    let code = status.code().unwrap_or(1) as u32;
    let success = code == 0;

    Ok(BuildResult {
        success,
        errors: if success { 0 } else { code },
        warnings: 0,
        build_time: String::new(),
        program_size: None,
        output_file: String::new(),
        log: String::new(),
    })
}

/// Wait for a running UV4, optionally echoing its log, and give up when it
/// stops making progress.
///
/// Wall-clock alone cannot tell "a slow build" from "a dialog nobody will
/// answer", and the caller is usually not a human who can look at the window.
/// CPU time separates them: a blocked process consumes none.
fn supervise(
    child: &mut Child,
    mut log: Option<&mut LogPoller>,
    opts: &BuildOptions,
) -> anyhow::Result<ExitStatus> {
    let started = Instant::now();
    let mut last_cpu: Option<u64> = None;
    let mut last_progress = Instant::now();

    loop {
        if let Some(status) = child.try_wait()? {
            if let Some(poller) = log.as_deref_mut() {
                poller.drain();
            }
            return Ok(status);
        }

        if let Some(poller) = log.as_deref_mut() {
            poller.drain();
        }

        if opts.timeout_secs > 0 && started.elapsed().as_secs() >= opts.timeout_secs {
            kill(child);
            anyhow::bail!(
                "UV4 was still running after {}s and was terminated (--timeout)",
                opts.timeout_secs
            );
        }

        if let Some(cpu) = process_cpu_time(child.id()) {
            if last_cpu != Some(cpu) {
                last_cpu = Some(cpu);
                last_progress = Instant::now();
            } else if last_progress.elapsed().as_secs() >= STALL_LIMIT_SECS {
                kill(child);
                anyhow::bail!(
                    "UV4 consumed no CPU for {STALL_LIMIT_SECS}s and was terminated - it is \
                     blocked, most likely on a modal dialog (-j0 hides the window, not the \
                     dialog) or on a licence checkout"
                );
            }
        }

        std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
    }
}

fn kill(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// CPU time consumed by a process so far, in 100 ns units.
///
/// `None` means "cannot tell", and the caller then relies on the wall-clock cap
/// alone rather than guessing.
#[cfg(target_os = "windows")]
fn process_cpu_time(pid: u32) -> Option<u64> {
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME};
    use windows_sys::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }

        let mut creation = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let mut exit = creation;
        let mut kernel = creation;
        let mut user = creation;
        let ok = GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user);
        CloseHandle(handle);

        if ok == 0 {
            return None;
        }
        Some(filetime_ticks(kernel) + filetime_ticks(user))
    }
}

#[cfg(target_os = "windows")]
fn filetime_ticks(t: windows_sys::Win32::Foundation::FILETIME) -> u64 {
    ((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64
}

#[cfg(not(target_os = "windows"))]
fn process_cpu_time(_pid: u32) -> Option<u64> {
    None
}

/// Echoes whatever the build log has grown by since the last poll.
///
/// Keil has no way to stream a build to stdout — the only output is the `-o`
/// log file — but it *writes* that file as it goes (measured on a real
/// rebuild: 4 bytes at start, 46 KB by the end, growing continuously).
/// Following it is therefore enough, and needs nothing from Keil.
struct LogPoller {
    path: PathBuf,
    offset: u64,
    enabled: bool,
}

impl LogPoller {
    fn new(path: &Path, enabled: bool) -> Self {
        Self {
            path: path.to_path_buf(),
            offset: 0,
            enabled,
        }
    }

    /// Forward every complete line written since the last call.
    ///
    /// Only whole lines are consumed: the bytes after the last newline are
    /// still being written and would otherwise be cut in half.
    fn drain(&mut self) {
        if !self.enabled {
            return;
        }
        let Ok(data) = std::fs::read(&self.path) else {
            return;
        };
        if data.len() as u64 <= self.offset {
            return;
        }

        let fresh = &data[self.offset as usize..];
        let Some(last_newline) = fresh.iter().rposition(|b| *b == b'\n') else {
            return;
        };
        let ready = &fresh[..=last_newline];
        self.offset += ready.len() as u64;

        // Progress belongs on stderr: stdout carries the command's result, and
        // mixing the two would break `--json` and every pipe that reads it.
        eprint!("{}", String::from_utf8_lossy(ready));
    }
}

/// The last `n` lines of a log, for error messages.
fn tail_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verbatim `reg query HKLM\SOFTWARE\WOW6432Node\Keil\Products\MDK /v Path`.
    const REG_HIT: &str = "\r\n\
HKEY_LOCAL_MACHINE\\SOFTWARE\\WOW6432Node\\Keil\\Products\\MDK\r\n\
    Path    REG_SZ    C:\\Users\\me\\AppData\\Local\\Keil_v5\\ARM\r\n\r\n";

    /// Verbatim output when the key is absent.
    const REG_MISS: &str = "\r\n\
ERROR: The system was unable to find the specified registry key or value.\r\n\r\n";

    #[test]
    fn parses_the_install_root_out_of_reg_query_output() {
        assert_eq!(
            parse_reg_path(REG_HIT).as_deref(),
            Some(r"C:\Users\me\AppData\Local\Keil_v5\ARM")
        );
    }

    #[test]
    fn registry_miss_yields_nothing() {
        assert_eq!(parse_reg_path(REG_MISS), None);
        assert_eq!(parse_reg_path(""), None);
    }

    /// The registry records the ARM toolchain directory; UV4 is its sibling.
    #[test]
    fn uv4_sits_beside_the_registered_arm_directory() {
        assert_eq!(
            uv4_beside_arm_dir(Path::new(r"C:\Users\me\AppData\Local\Keil_v5\ARM")).as_deref(),
            Some(Path::new(r"C:\Users\me\AppData\Local\Keil_v5\UV4\UV4.exe"))
        );
        assert_eq!(uv4_beside_arm_dir(Path::new("ARM")), None);
    }
}
