pub mod parser;
pub mod editor;
pub mod generator;

use crate::output::{self, OutputValue, OutputFormat};
use parser::load_ioc;
use std::path::Path;

pub fn handle(ioc: &super::IocCommands, cfg: &crate::config::EmbConfig, format: OutputFormat) -> anyhow::Result<()> {
    match ioc {
        super::IocCommands::Info { path } => cmd_info(path, format),
        super::IocCommands::Get { path, prefix } => cmd_get(path, prefix, format),
        super::IocCommands::Set { path, key, value } => cmd_set(path, key, value, format),
        super::IocCommands::Rm { path, key } => cmd_rm(path, key, format),
        super::IocCommands::Generate { path, cubemx } => cmd_generate(path, cubemx, cfg, format),
    }
}

fn cmd_info(path: &str, format: OutputFormat) -> anyhow::Result<()> {
    let ioc = load_ioc(std::path::Path::new(path))?;

    // Count entries per category
    let mut counts = std::collections::HashMap::<&str, usize>::new();
    for (key, _) in &ioc.entries {
        let category = key.split('.').next().unwrap_or(key);
        *counts.entry(category).or_default() += 1;
    }

    let rows: Vec<Vec<String>> = ioc
        .categories
        .iter()
        .map(|cat| {
            let count = counts.get(cat.as_str()).copied().unwrap_or(0);
            vec![cat.clone(), count.to_string()]
        })
        .collect();

    let value = OutputValue::Table {
        headers: vec!["Category".into(), "Entries".into()],
        rows,
    };
    output::display(&value, format);
    Ok(())
}

fn cmd_get(path: &str, prefix: &str, format: OutputFormat) -> anyhow::Result<()> {
    let ioc = load_ioc(Path::new(path))?;

    // Exact match takes priority
    if let Some(val) = ioc.get(prefix) {
        let value = OutputValue::Message(val.to_string());
        output::display(&value, format);
        return Ok(());
    }

    // Otherwise, prefix match
    let matched = ioc.get_by_prefix(prefix);
    if matched.is_empty() {
        anyhow::bail!("No entries found matching prefix: {prefix}");
    }

    let pairs: Vec<(String, String)> = matched
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let value = OutputValue::KeyValue(pairs);
    output::display(&value, format);
    Ok(())
}

fn cmd_set(path: &str, key: &str, value: &str, format: OutputFormat) -> anyhow::Result<()> {
    editor::set(Path::new(path), key, value)?;
    let value = OutputValue::Message("ok".to_string());
    output::display(&value, format);
    Ok(())
}

fn cmd_rm(path: &str, key: &str, format: OutputFormat) -> anyhow::Result<()> {
    editor::remove(Path::new(path), key)?;
    let value = OutputValue::Message("ok".to_string());
    output::display(&value, format);
    Ok(())
}

fn cmd_generate(path: &str, cubemx: &Option<String>, cfg: &crate::config::EmbConfig, format: OutputFormat) -> anyhow::Result<()> {
    let explicit = cubemx.as_deref().map(Path::new);
    let resolved = crate::config::resolve_cubemx(cfg, explicit)?;
    generator::generate(Path::new(path), Some(&resolved))?;
    let value = OutputValue::Message("ok".to_string());
    output::display(&value, format);
    Ok(())
}
