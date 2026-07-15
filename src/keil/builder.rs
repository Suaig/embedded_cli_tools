use std::path::{Path, PathBuf};
use std::process::Command;

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
/// 3. Common fixed paths
/// 4. Scan `C:\Program Files` and `C:\Program Files (x86)` for Keil directories
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

    // 3. Common fixed paths
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
pub fn build(
    path: &Path,
    target: &Option<String>,
    command: BuildCommand,
    uv4_path: Option<PathBuf>,
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
        return run_clean(&uv4, path, target);
    }

    // Create a temp file for the build log.
    let temp_dir = std::env::temp_dir();
    let log_file = temp_dir.join(format!("emb_build_{}.log", uuid::Uuid::new_v4().simple()));

    let result = run_uv4(&uv4, path, target, command, &log_file);

    // Read the log regardless of exit status.
    let log_content = std::fs::read_to_string(&log_file).unwrap_or_default();

    // Clean up temp log file (best-effort).
    let _ = std::fs::remove_file(&log_file);

    let exit_code = result?;

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

    let status = cmd
        .status()
        .with_context(|| format!("failed to execute UV4 at {}", uv4.display()))?;

    // UV4 exit code semantics: 0=success, 1=warnings, 2+=errors, 20=special errors
    Ok(status.code().unwrap_or(1) as u32)
}

fn run_clean(
    uv4: &Path,
    project: &Path,
    target: &Option<String>,
) -> anyhow::Result<BuildResult> {
    let mut cmd = Command::new(uv4);
    cmd.arg("-j0");
    cmd.arg("-c");

    let abs_project = std::path::absolute(project)?;
    cmd.arg(abs_project);

    if let Some(t) = target {
        cmd.arg("-t").arg(t);
    }

    let status = cmd.status()
        .with_context(|| format!("failed to execute UV4 at {}", uv4.display()))?;

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
