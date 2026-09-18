use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

/// Generate full project from IOC file using STM32CubeMX CLI script mode.
///
/// CubeMX `-q` flag requires a script file (not the IOC path directly).
/// STM32CubeMX.exe is a launcher that spawns a JVM and returns immediately,
/// so we must wait for the actual generation to complete by polling the output.
pub fn generate(
    path: &Path,
    cubemx_path: Option<&Path>,
    timeout_secs: u64,
) -> anyhow::Result<()> {
    // Checked before launching anything: CubeMX is a heavyweight JVM, and
    // handed a missing .ioc it dies with a NullPointerException only after the
    // full timeout has elapsed.
    if !path.is_file() {
        anyhow::bail!("ioc file not found: {}", path.display());
    }

    let exe = match cubemx_path {
        Some(p) => p.to_path_buf(),
        None => find_cubemx()?,
    };

    if !exe.exists() {
        anyhow::bail!(
            "STM32CubeMX not found at: {}",
            exe.display()
        );
    }

    let project_dir = project_dir_of(path);
    let project_name = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let uvprojx_path = project_dir
        .join("MDK-ARM")
        .join(format!("{}.uvprojx", project_name));

    // Write CubeMX CLI script to temp dir (not project dir, to avoid cleanup issues)
    let script_content = format!(
        "config load {}\nproject name {}\nproject toolchain \"MDK-ARM\"\nproject path {}\nproject generate\nexit\n",
        path.display(),
        project_name,
        project_dir.display(),
    );

    // Unique per run so that the completion poll and the "still running" report
    // below can only ever refer to this invocation's CubeMX process.
    let script_path = std::env::temp_dir().join(format!(
        "emb_cubemx_{}_{}.txt",
        project_name,
        uuid::Uuid::new_v4().simple()
    ));
    fs::write(&script_path, &script_content)?;

    // Launch CubeMX (launcher returns immediately, JVM runs in background)
    Command::new(&exe)
        .arg("-q")
        .arg(&script_path)
        .current_dir(project_dir)
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to execute STM32CubeMX: {e}"))?;

    // Wait for .uvprojx to appear AND be fully written (CubeMX JVM generates it)
    let timeout = Duration::from_secs(timeout_secs);
    let start = Instant::now();
    let mut found = false;

    while start.elapsed() < timeout {
        std::thread::sleep(Duration::from_secs(1));
        if uvprojx_path.exists() {
            // Check file is complete (has closing </Project> tag)
            if let Ok(content) = fs::read_to_string(&uvprojx_path) {
                if content.contains("</Project>") {
                    found = true;
                    break;
                }
            }
        }
    }

    // `-q` does not dismiss CubeMX dialogs, so a run that never produced the
    // .uvprojx may still be alive and waiting for a human. Report it rather than
    // killing it: a dialog the user is about to answer is one click away from
    // succeeding, and killing it would throw that work away.
    let lingering = if found {
        Vec::new()
    } else {
        find_cubemx_pids(&script_path)
    };

    // Clean up temp script
    let _ = fs::remove_file(&script_path);

    if !found {
        anyhow::bail!("{}", timeout_message(timeout.as_secs(), &lingering));
    }

    // Fix CubeMX path bugs in generated .uvprojx
    fix_cubemx_path_bugs(project_dir, &project_name);

    Ok(())
}

/// What to tell the user when CubeMX produced nothing within the deadline.
///
/// A live process means "still waiting on a dialog", not "failed": CubeMX's `-q`
/// script mode does not dismiss its own confirmation dialogs, so the run cannot
/// finish until a human answers. Only the caller can decide what to do about
/// that, so the message says what is actually happening and what the options
/// are, rather than declaring a failure.
fn timeout_message(timeout_secs: u64, lingering_pids: &[u32]) -> String {
    if lingering_pids.is_empty() {
        return format!(
            "STM32CubeMX produced nothing within {timeout_secs}s and is no longer running. \
             Check ~/.stm32cubemx/STM32CubeMX.log for what it did."
        );
    }

    let pids: Vec<String> = lingering_pids.iter().map(u32::to_string).collect();
    format!(
        "STM32CubeMX is still running (PID {}) after {timeout_secs}s, most likely showing a \
         confirmation dialog: `-q` script mode does not dismiss it, so only a human (or an \
         agent driving the GUI) can. Answer it and re-run, raise `--timeout <seconds>`, or \
         stop it with `taskkill {}`.",
        pids.join(", "),
        pids.iter()
            .map(|p| format!("/PID {p} /F"))
            .collect::<Vec<_>>()
            .join(" ")
    )
}

/// The directory to run the CubeMX launcher in.
///
/// `Path::new("Fan.ioc").parent()` is `Some("")`, and `Command::current_dir("")`
/// fails with ERROR_INVALID_NAME on Windows, so a bare filename must fall back to
/// the current directory.
fn project_dir_of(path: &Path) -> &Path {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    }
}

/// PowerShell that lists the PIDs of processes whose command line mentions
/// `script_path` — that is the CubeMX JVM the launcher started for this run.
#[cfg(target_os = "windows")]
fn find_cubemx_pids_command(script_path: &Path) -> String {
    format!(
        "(Get-CimInstance Win32_Process | Where-Object {{ $_.CommandLine -like '*{}*' }}).ProcessId",
        script_path.display()
    )
}

#[cfg(target_os = "windows")]
fn find_cubemx_pids(script_path: &Path) -> Vec<u32> {
    let Ok(output) = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &find_cubemx_pids_command(script_path),
        ])
        .output()
    else {
        return Vec::new();
    };

    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .filter_map(|token| token.parse().ok())
        .collect()
}

#[cfg(not(target_os = "windows"))]
fn find_cubemx_pids(_script_path: &Path) -> Vec<u32> {
    Vec::new()
}

/// Fix known CubeMX path bugs in the generated .uvprojx:
/// 1. File paths like `../D:/Desktop/...` should be `../Core/Src/...`
/// 2. Duplicate groups with absolute paths in group name
fn fix_cubemx_path_bugs(project_dir: &Path, project_name: &str) {
    let uvprojx_path =
        project_dir.join("MDK-ARM").join(format!("{}.uvprojx", project_name));
    if !uvprojx_path.exists() {
        return;
    }

    let Ok(content) = fs::read_to_string(&uvprojx_path) else {
        return;
    };

    let fixed = fix_buggy_paths(&content);

    if fixed != content {
        let _ = fs::write(&uvprojx_path, &fixed);
    }
}

/// Fix `../<absolute-path>/<relative-path>` in XML FilePath elements.
/// e.g. `../D:/Desktop/project/Core/Src/main.c` -> `../Core/Src/main.c`
fn fix_buggy_paths(content: &str) -> String {
    // Extract all absolute path prefixes from the content.
    // Pattern: ../<drive>:/<path>/ where <path> contains at least one /
    // Then replace ../<drive>:/<path>/Core/Src/ with ../Core/Src/ etc.
    let prefixes = ["Core/Src/", "Core/Inc/", "Drivers/"];
    let mut result = content.to_string();

    // Find all ../<letter>:/.../ patterns and build a set of bad prefixes
    let mut bad_prefixes: Vec<String> = Vec::new();
    let mut scan = result.as_str();
    while let Some(pos) = scan.find("../") {
        let rest = &scan[pos + 3..];
        // Check if this starts with a drive letter path
        if rest.len() > 3 && rest.as_bytes()[1] == b':' && (rest.as_bytes()[2] == b'/' || rest.as_bytes()[2] == b'\\') {
            // This is a buggy path. Find the end of the absolute portion.
            // The absolute path ends when we hit one of our known prefixes
            for prefix in &prefixes {
                if let Some(idx) = rest.find(prefix) {
                    let abs_part = &rest[..idx];
                    let bad = format!("../{}", abs_part);
                    if !bad_prefixes.contains(&bad) {
                        bad_prefixes.push(bad);
                    }
                }
            }
        }
        scan = rest;
    }

    // Replace all bad prefixes with just "../"
    for bad in &bad_prefixes {
        result = result.replace(bad, "../");
    }

    remove_abs_path_groups(&result)
}

/// Remove <Group> blocks whose GroupName contains an absolute path (drive letter).
fn remove_abs_path_groups(content: &str) -> String {
    let mut result = String::with_capacity(content.len());
    let mut i = 0;

    while i < content.len() {
        if content[i..].starts_with("<Group>") {
            if let Some(end) = content[i..].find("</Group>") {
                let block_end = i + end + 8;
                let block = &content[i..block_end];
                let is_dup = block.contains("<GroupName>")
                    && block.contains("</GroupName>")
                    && {
                        let start = block.find("<GroupName>").unwrap() + 11;
                        let end = block.find("</GroupName>").unwrap();
                        let name = &block[start..end];
                        name.contains(":/") || name.contains(":\\")
                    };
                if is_dup {
                    i = block_end;
                    continue;
                }
                result.push_str(block);
                i = block_end;
                continue;
            }
        }
        result.push(content.as_bytes()[i] as char);
        i += 1;
    }

    result
}

/// Auto-detect STM32CubeMX executable path.
pub fn find_cubemx() -> anyhow::Result<PathBuf> {
    if let Ok(env_path) = std::env::var("CUBEMX_PATH") {
        return Ok(PathBuf::from(env_path));
    }

    let mut candidates = vec![
        PathBuf::from(r"C:\Program Files\STMicroelectronics\STM32Cube\STM32CubeMX\STM32CubeMX.exe"),
        PathBuf::from(r"C:\St\STM32CubeMX\STM32CubeMX.exe"),
    ];

    // The standalone installer defaults to a per-user location.
    if let Some(base) = std::env::var_os("LOCALAPPDATA") {
        let base = PathBuf::from(base);
        candidates.push(base.join("Programs").join("STM32CubeMX").join("STM32CubeMX.exe"));
        candidates.push(base.join("STM32CubeMX").join("STM32CubeMX.exe"));
    }

    for p in &candidates {
        if p.is_file() {
            return Ok(p.clone());
        }
    }

    anyhow::bail!(
        "STM32CubeMX not found. Use --cubemx <path>, set CUBEMX_PATH, or run `emb config set cubemx_path <path>`."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_fix_absolute() {
        let input = r#"<FilePath>../D:/Desktop/signal_project/Core/Src/main.c</FilePath>"#;
        let fixed = fix_buggy_paths(input);
        assert!(fixed.contains("../Core/Src/main.c"));
        assert!(!fixed.contains("D:/Desktop"));
    }

    #[test]
    fn test_path_fix_relative_unchanged() {
        let input = r#"<FilePath>../Core/Src/main.c</FilePath>"#;
        let fixed = fix_buggy_paths(input);
        assert_eq!(fixed, input);
    }

    #[test]
    fn test_path_fix_drivers() {
        let input = r#"<FilePath>../D:/proj/Drivers/STM32H7xx_HAL_Driver/Src/hal.c</FilePath>"#;
        let fixed = fix_buggy_paths(input);
        assert!(fixed.contains("../Drivers/STM32H7xx_HAL_Driver/Src/hal.c"));
    }

    #[test]
    fn test_remove_abs_path_groups() {
        let input = r#"<Group><GroupName>Application/User/Core</GroupName></Group><Group><GroupName>Application/User/D:/Desktop/proj/Core</GroupName></Group>"#;
        let fixed = remove_abs_path_groups(input);
        assert!(fixed.contains("Application/User/Core"));
        assert!(!fixed.contains("D:/Desktop"));
    }

    /// `Path::new("x.ioc").parent()` is `Some("")`, and `Command::current_dir("")`
    /// fails with ERROR_INVALID_NAME on Windows, so a bare filename must fall back
    /// to the current directory.
    #[test]
    fn bare_filename_uses_the_current_directory() {
        assert_eq!(project_dir_of(Path::new("Fan.ioc")), Path::new("."));
    }

    #[test]
    fn nested_path_uses_its_parent() {
        assert_eq!(project_dir_of(Path::new("sub/Fan.ioc")), Path::new("sub"));
        assert_eq!(
            project_dir_of(Path::new(r"C:\proj\Fan.ioc")),
            Path::new(r"C:\proj")
        );
    }

    /// The process lookup must key off this run's unique script path, otherwise
    /// it could report — or stop — an unrelated CubeMX session.
    #[cfg(target_os = "windows")]
    #[test]
    fn cubemx_lookup_keys_off_this_runs_script_path() {
        let script = Path::new(r"C:\Temp\emb_cubemx_Fan_9f3c1a.txt");
        let cmd = find_cubemx_pids_command(script);
        assert!(cmd.contains(r"C:\Temp\emb_cubemx_Fan_9f3c1a.txt"), "{cmd}");
        assert!(cmd.contains("ProcessId"), "{cmd}");
    }

    /// Reporting "timed out" while CubeMX is merely waiting on a human dialog
    /// sends the user hunting for a bug that does not exist.
    #[test]
    fn a_live_cubemx_process_is_reported_as_waiting() {
        let msg = timeout_message(120, &[4242]);
        assert!(msg.contains("4242"), "{msg}");
        assert!(
            msg.contains("--timeout"),
            "should suggest raising the deadline: {msg}"
        );
        assert!(msg.contains("taskkill"), "should say how to stop it: {msg}");
    }

    #[test]
    fn a_dead_cubemx_process_points_at_its_log() {
        let msg = timeout_message(120, &[]);
        assert!(msg.contains("STM32CubeMX.log"), "{msg}");
        assert!(
            !msg.contains("taskkill"),
            "nothing left to stop: {msg}"
        );
    }
}
