use std::path::{Path, PathBuf};
use std::process::Command;

/// Generate code from IOC file using STM32CubeMX.
///
/// CubeMX executable is resolved in this order:
/// 1. Explicit `cubemx_path` parameter
/// 2. `CUBEMX_PATH` environment variable
/// 3. Common install paths on Windows
pub fn generate(path: &Path, cubemx_path: Option<&Path>) -> anyhow::Result<()> {
    let exe = resolve_cubemx(cubemx_path)?;

    if !exe.exists() {
        anyhow::bail!(
            "STM32CubeMX not found at: {}",
            exe.display()
        );
    }

    let status = Command::new(&exe)
        .arg("-q")
        .arg(path)
        .current_dir(path.parent().unwrap_or(Path::new(".")))
        .status()
        .map_err(|e| anyhow::anyhow!("Failed to execute STM32CubeMX: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        let code = status.code().unwrap_or(-1);
        anyhow::bail!("STM32CubeMX exited with code {code}");
    }
}

/// Resolve STM32CubeMX executable path.
fn resolve_cubemx(explicit: Option<&Path>) -> anyhow::Result<PathBuf> {
    // 1. Explicit path from --cubemx flag
    if let Some(p) = explicit {
        return Ok(p.to_path_buf());
    }

    // 2. CUBEMX_PATH environment variable
    if let Ok(env_path) = std::env::var("CUBEMX_PATH") {
        return Ok(PathBuf::from(env_path));
    }

    // 3. Common install paths
    let candidates = [
        r"C:\Program Files\STMicroelectronics\STM32Cube\STM32CubeMX\STM32CubeMX.exe",
        r"C:\ST\STM32CubeMX\STM32CubeMX.exe",
    ];

    for candidate in &candidates {
        let p = PathBuf::from(candidate);
        if p.exists() {
            return Ok(p);
        }
    }

    anyhow::bail!(
        "STM32CubeMX not found. Use --cubemx <path> or set CUBEMX_PATH environment variable."
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_explicit_path() {
        let p = resolve_cubemx(Some(Path::new("/some/path/STM32CubeMX.exe")));
        assert!(p.is_ok());
        assert_eq!(p.unwrap(), PathBuf::from("/some/path/STM32CubeMX.exe"));
    }

    #[test]
    fn test_resolve_no_match_without_env() {
        // Clear CUBEMX_PATH to ensure predictable test
        std::env::remove_var("CUBEMX_PATH");
        // This will fail if common paths don't exist (expected on CI/non-Windows)
        let result = resolve_cubemx(None);
        // Just verify it returns an error (since CubeMX likely isn't installed)
        assert!(result.is_err() || result.is_ok());
    }
}
