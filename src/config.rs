use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const CONFIG_FILE_NAME: &str = ".embconfig.toml";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EmbConfig {
    pub keil_path: Option<String>,
    pub cubemx_path: Option<String>,
}

/// Get the user-level config file path (priority 1).
fn user_config_path() -> Option<PathBuf> {
    dirs_home_dir().map(|d| d.join(CONFIG_FILE_NAME))
}

/// Get the cwd-level config file path (priority 2).
fn cwd_config_path() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_default()
        .join(CONFIG_FILE_NAME)
}

/// Cross-platform home directory without extra deps.
fn dirs_home_dir() -> Option<PathBuf> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()
        .map(PathBuf::from)
}

/// Load merged config: user home first, then cwd overrides.
pub fn load() -> EmbConfig {
    let mut config = EmbConfig::default();

    // Priority 2: cwd config (loaded first, lower priority)
    if let Ok(cwd_config) = load_from_file(&cwd_config_path()) {
        config = cwd_config;
    }

    // Priority 1: user home config (overrides cwd)
    if let Some(user_path) = user_config_path() {
        if let Ok(user_config) = load_from_file(&user_path) {
            if user_config.keil_path.is_some() {
                config.keil_path = user_config.keil_path;
            }
            if user_config.cubemx_path.is_some() {
                config.cubemx_path = user_config.cubemx_path;
            }
        }
    }

    config
}

fn load_from_file(path: &Path) -> Result<EmbConfig> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read config: {}", path.display()))?;
    let config: EmbConfig = toml::from_str(&content)
        .with_context(|| format!("failed to parse config: {}", path.display()))?;
    Ok(config)
}

/// Save a config value. `global = true` saves to user home, otherwise cwd.
pub fn set(key: &str, value: &str, global: bool) -> Result<PathBuf> {
    let path = if global {
        user_config_path()
            .context("cannot determine user home directory")?
    } else {
        cwd_config_path()
    };

    let mut config = load_from_file(&path).unwrap_or_default();

    match key {
        "keil_path" => config.keil_path = Some(value.to_string()),
        "cubemx_path" => config.cubemx_path = Some(value.to_string()),
        _ => anyhow::bail!(
            "unknown config key '{}'. Valid keys: keil_path, cubemx_path",
            key
        ),
    }

    let content = toml::to_string_pretty(&config)
        .context("failed to serialize config")?;
    std::fs::write(&path, content)
        .with_context(|| format!("failed to write config: {}", path.display()))?;

    Ok(path)
}

/// Remove a config value.
pub fn unset(key: &str, global: bool) -> Result<PathBuf> {
    let path = if global {
        user_config_path()
            .context("cannot determine user home directory")?
    } else {
        cwd_config_path()
    };

    if !path.exists() {
        anyhow::bail!("config file not found: {}", path.display());
    }

    let mut config = load_from_file(&path)?;

    match key {
        "keil_path" => config.keil_path = None,
        "cubemx_path" => config.cubemx_path = None,
        _ => anyhow::bail!(
            "unknown config key '{}'. Valid keys: keil_path, cubemx_path",
            key
        ),
    }

    let content = toml::to_string_pretty(&config)
        .context("failed to serialize config")?;
    std::fs::write(&path, content)
        .with_context(|| format!("failed to write config: {}", path.display()))?;

    Ok(path)
}

/// Get the effective resolved path for UV4.exe.
/// Checks config first, then falls back to builder::find_uv4() auto-detection.
pub fn resolve_uv4(config: &EmbConfig) -> Result<std::path::PathBuf> {
    if let Some(ref p) = config.keil_path {
        let path = std::path::PathBuf::from(p);
        if path.exists() {
            return Ok(path);
        }
        anyhow::bail!("keil_path in config does not exist: {}", p);
    }
    crate::keil::builder::find_uv4()
}

/// Get the effective resolved path for STM32CubeMX.
/// Checks config first, then falls back to generator auto-detection.
pub fn resolve_cubemx(config: &EmbConfig, explicit: Option<&Path>) -> Result<std::path::PathBuf> {
    if let Some(p) = explicit {
        return Ok(p.to_path_buf());
    }
    if let Some(ref p) = config.cubemx_path {
        let path = std::path::PathBuf::from(p);
        if path.exists() {
            return Ok(path);
        }
        anyhow::bail!("cubemx_path in config does not exist: {}", p);
    }
    crate::ioc::generator::find_cubemx()
}
