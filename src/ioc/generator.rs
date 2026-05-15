use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

/// Generate full project from IOC file using STM32CubeMX CLI script mode.
///
/// CubeMX `-q` flag requires a script file (not the IOC path directly).
/// STM32CubeMX.exe is a launcher that spawns a JVM and returns immediately,
/// so we must wait for the actual generation to complete by polling the output.
pub fn generate(path: &Path, cubemx_path: Option<&Path>) -> anyhow::Result<()> {
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

    let project_dir = path.parent().unwrap_or(Path::new("."));
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

    let script_path = std::env::temp_dir().join(format!("emb_cubemx_{}.txt", project_name));
    fs::write(&script_path, &script_content)?;

    // Launch CubeMX (launcher returns immediately, JVM runs in background)
    Command::new(&exe)
        .arg("-q")
        .arg(&script_path)
        .current_dir(project_dir)
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to execute STM32CubeMX: {e}"))?;

    // Wait for .uvprojx to appear AND be fully written (CubeMX JVM generates it)
    let timeout = Duration::from_secs(120);
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

    // Clean up temp script
    let _ = fs::remove_file(&script_path);

    if !found {
        anyhow::bail!(
            "STM32CubeMX generation timed out after {}s. Check ~/.stm32cubemx/STM32CubeMX.log for errors.",
            timeout.as_secs()
        );
    }

    // Fix CubeMX path bugs in generated .uvprojx
    fix_cubemx_path_bugs(project_dir, &project_name);

    Ok(())
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

    let candidates = [
        r"C:\Program Files\STMicroelectronics\STM32Cube\STM32CubeMX\STM32CubeMX.exe",
        r"C:\St\STM32CubeMX\STM32CubeMX.exe",
    ];

    for candidate in &candidates {
        let p = PathBuf::from(candidate);
        if p.exists() {
            return Ok(p);
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
}
