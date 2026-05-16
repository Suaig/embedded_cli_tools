pub mod parser;
pub mod editor;
pub mod builder;

pub use parser::{
    load_project, load_workspace,
    is_workspace_file, is_project_file,
    KeilProject, KeilWorkspace, Target,
};

use std::path::PathBuf;

use crate::output::{self, OutputFormat, OutputValue};

use super::Cli;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert numeric file type to human-readable string.
fn file_type_str(ft: u8) -> &'static str {
    match ft {
        1 => "C",
        2 => "Asm",
        3 => "Obj",
        4 => "Lib",
        5 => "Header",
        6 => "Text",
        _ => "Unknown",
    }
}

/// Optimization level: current value + full option map (differs by AC5/AC6).
fn optim_str(level: u8, ac6: bool) -> String {
    if ac6 {
        const OPTIONS: &str = "[0=default 1=O0 2=O1 3=O2 4=O3 5=Ofast 6=Os 7=Oz 8=Omax]";
        let label = match level {
            0 => "default", 1 => "O0", 2 => "O1", 3 => "O2",
            4 => "O3", 5 => "Ofast", 6 => "Os", 7 => "Oz", 8 => "Omax",
            _ => "?",
        };
        format!("{level} ({label}) {OPTIONS}")
    } else {
        const OPTIONS: &str = "[0=O0 1=O1 2=O2 3=O3] (use oTime for size/speed)";
        let label = match level {
            0 => "O0", 1 => "O1", 2 => "O2", 3 => "O3",
            _ => "?",
        };
        format!("{level} ({label}) {OPTIONS}")
    }
}

/// Warning level: current value + full option map.
fn wlevel_str(level: u8) -> String {
    const OPTIONS: &str = "[0=None 1=Low 2=Medium 3=High]";
    let label = match level {
        0 => "None",
        1 => "Low",
        2 => "Medium",
        3 => "High",
        _ => "?",
    };
    format!("{level} ({label}) {OPTIONS}")
}

/// v6Lang: C language standard for AC6 + full option map.
fn v6lang_str(val: u8) -> String {
    const OPTIONS: &str = "[0=auto 1=c90 2=gnu90 3=c99 4=gnu99 5=c11 6=gnu11]";
    let label = match val {
        0 => "auto", 1 => "c90", 2 => "gnu90",
        3 => "c99", 4 => "gnu99", 5 => "c11", 6 => "gnu11",
        _ => "?",
    };
    format!("{val} ({label}) {OPTIONS}")
}

/// v6LangP: C++ language profile for AC6 + full option map.
fn v6langp_str(val: u8) -> String {
    const OPTIONS: &str = "[0=auto 1=c++98 2=gnu++98 3=c++11 4=gnu++11 5=c++14 6=gnu++14]";
    let label = match val {
        0 => "auto", 1 => "c++98", 2 => "gnu++98",
        3 => "c++11", 4 => "gnu++11", 5 => "c++14", 6 => "gnu++14",
        _ => "?",
    };
    format!("{val} ({label}) {OPTIONS}")
}

/// Resolve a target by name, falling back to the first target if `name` is None.
fn resolve_target<'a>(project: &'a KeilProject, name: &Option<String>) -> anyhow::Result<&'a Target> {
    match name {
        Some(n) => project
            .targets
            .iter()
            .find(|t| t.name == *n)
            .ok_or_else(|| anyhow::anyhow!("target '{}' not found (available: {})", n,
                project.targets.iter().map(|t| &t.name).cloned().collect::<Vec<_>>().join(", "))),
        None => project
            .targets
            .first()
            .ok_or_else(|| anyhow::anyhow!("project has no targets")),
    }
}

/// For workspace files: resolve `-p <project>` to the actual .uvprojx path.
/// Returns `(resolved_uvprojx_path, was_workspace)`.
fn resolve_project_path(
    path_str: &str,
    project_filter: &Option<String>,
) -> anyhow::Result<(PathBuf, bool)> {
    let path = PathBuf::from(path_str);
    if is_workspace_file(&path) {
        let ws = load_workspace(&path)?;
        let ws_dir = path.parent().ok_or_else(|| anyhow::anyhow!("cannot determine workspace directory"))?;

        let wp = match project_filter {
            Some(p) => {
                // Match by full path or by filename
                ws.projects
                    .iter()
                    .find(|proj| {
                        proj.path == *p
                            || PathBuf::from(&proj.path)
                                .file_name()
                                .and_then(|n| n.to_str())
                                .map(|n| n == p.as_str())
                                .unwrap_or(false)
                    })
                    .ok_or_else(|| {
                        let available: Vec<String> =
                            ws.projects.iter().map(|proj| proj.path.clone()).collect();
                        anyhow::anyhow!(
                            "project '{}' not found in workspace (available: {})",
                            p,
                            available.join(", ")
                        )
                    })?
            }
            None => ws
                .projects
                .first()
                .ok_or_else(|| anyhow::anyhow!("workspace has no projects"))?,
        };

        let resolved = ws_dir.join(&wp.path);
        Ok((resolved, true))
    } else if is_project_file(&path) {
        Ok((path, false))
    } else {
        anyhow::bail!(
            "unsupported file type: {}. Expected .uvprojx or .uvmpw",
            path.display()
        );
    }
}

// ---------------------------------------------------------------------------
// Command handlers
// ---------------------------------------------------------------------------

fn cmd_info_project(
    project: &KeilProject,
    target_name: &Option<String>,
    format: OutputFormat,
) -> anyhow::Result<()> {
    match target_name {
        Some(name) => {
            // Detailed summary for a specific target
            let t = resolve_target(project, &Some(name.clone()))?;
            let total_files: usize = t.groups.iter().map(|g| g.files.len()).sum();
            let pairs = vec![
                ("Target".into(), t.name.clone()),
                ("Device".into(), t.device.name.clone()),
                ("Output Name".into(), t.output.name.clone()),
                ("Output Directory".into(), t.output.directory.clone()),
                ("Create Hex".into(), if t.output.create_hex { "yes" } else { "no" }.into()),
                ("Debug Info".into(), if t.output.debug_information { "yes" } else { "no" }.into()),
                ("Browse Info".into(), if t.output.browse_information { "yes" } else { "no" }.into()),
                ("Compiler Optim".into(), optim_str(t.c_compiler.optimization, t.ac6)),
                ("AC6".into(), if t.ac6 { "yes" } else { "no" }.into()),
                ("Toolset".into(), format!("{} ({})", t.toolset_name, t.toolset_number)),
                ("Defines".into(), t.c_compiler.defines.len().to_string()),
                ("Include Paths".into(), t.c_compiler.include_paths.len().to_string()),
                ("Groups".into(), t.groups.len().to_string()),
                ("Files".into(), total_files.to_string()),
                ("Include In Build".into(), if t.include_in_build { "yes" } else { "no" }.into()),
            ];
            output::display(&OutputValue::KeyValue(pairs), format);
        }
        None => {
            // List all targets
            let headers = vec![
                "Target".into(),
                "Device".into(),
                "Toolset".into(),
                "Include In Build".into(),
            ];
            let rows: Vec<Vec<String>> = project
                .targets
                .iter()
                .map(|t| {
                    vec![
                        t.name.clone(),
                        t.device.name.clone(),
                        t.toolset_name.clone(),
                        if t.include_in_build { "yes" } else { "no" }.into(),
                    ]
                })
                .collect();
            output::display(&OutputValue::Table { headers, rows }, format);
        }
    }
    Ok(())
}

fn cmd_info_workspace(
    ws: &KeilWorkspace,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let headers = vec![
        "Project".into(),
        "Active".into(),
        "Batch Build".into(),
    ];
    let rows: Vec<Vec<String>> = ws
        .projects
        .iter()
        .map(|p| {
            vec![
                p.path.clone(),
                if p.is_active { "yes" } else { "no" }.into(),
                if p.checked_in_batch_build { "yes" } else { "no" }.into(),
            ]
        })
        .collect();
    output::display(&OutputValue::Table { headers, rows }, format);
    Ok(())
}

fn cmd_info(
    path_str: &str,
    target: &Option<String>,
    project: &Option<String>,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let path = PathBuf::from(path_str);

    if is_workspace_file(&path) {
        if target.is_some() {
            // Workspace + target: resolve project then show target detail
            let (resolved, _) = resolve_project_path(path_str, project)?;
            let proj = load_project(&resolved)?;
            cmd_info_project(&proj, target, format)
        } else {
            let ws = load_workspace(&path)?;
            cmd_info_workspace(&ws, format)
        }
    } else if is_project_file(&path) {
        let proj = load_project(&path)?;
        cmd_info_project(&proj, target, format)
    } else {
        anyhow::bail!(
            "unsupported file type: {}. Expected .uvprojx or .uvmpw",
            path.display()
        );
    }
}

fn display_config(t: &Target, category: &Option<String>, format: OutputFormat) {
    let bool_str = |v: bool| -> String { if v { "yes".into() } else { "no".into() } };

    let mut pairs: Vec<(String, String)> = vec![
        ("device.name".into(), t.device.name.clone()),
        ("output.name".into(), t.output.name.clone()),
        ("output.hex".into(), bool_str(t.output.create_hex)),
        ("output.debug_info".into(), bool_str(t.output.debug_information)),
        ("ccompiler.ac6".into(), {
            let ty = if t.ac6 { "AC6" } else { "AC5" };
            format!("{ty} [yes=AC6 no=AC5]")
        }),
        ("ccompiler.pcc".into(), {
            if t.pcc.is_empty() {
                "(empty, uses default)".into()
            } else {
                let hint = if t.ac6 { "AC6" } else { "AC5" };
                format!("{} ({} compiler, format: <id>::<version>::<tool>)", t.pcc, hint)
            }
        }),
        ("ccompiler.optim".into(), optim_str(t.c_compiler.optimization, t.ac6)),
        ("ccompiler.otime".into(), bool_str(t.c_compiler.optimize_time)),
        ("ccompiler.c99".into(), bool_str(t.c_compiler.c99)),
        ("ccompiler.gnu".into(), bool_str(t.c_compiler.gnu)),
        ("ccompiler.wlevel".into(), wlevel_str(t.c_compiler.warning_level)),
        ("ccompiler.strict".into(), bool_str(t.c_compiler.strict)),
        ("ccompiler.one_elf".into(), bool_str(t.c_compiler.one_elf)),
        ("ccompiler.ropi".into(), bool_str(t.c_compiler.ropi)),
        ("ccompiler.rwpi".into(), bool_str(t.c_compiler.rwpi)),
        ("ccompiler.v6lang".into(), v6lang_str(t.c_compiler.lang)),
        ("ccompiler.v6langp".into(), v6langp_str(t.c_compiler.lang_profile)),
        ("ccompiler.short_enums".into(), bool_str(t.c_compiler.short_enums)),
        ("ccompiler.short_wchar".into(), bool_str(t.c_compiler.short_wchar)),
        ("ccompiler.misc".into(), t.c_compiler.misc_controls.clone()),
        ("asm.misc".into(), t.assembler.misc_controls.clone()),
        ("linker.scatter".into(), t.linker.scatter_file.clone()),
        ("linker.misc".into(), t.linker.misc.clone()),
        ("memory.irom.start".into(), t.memory.irom.start.clone()),
        ("memory.irom.size".into(), t.memory.irom.size.clone()),
        ("memory.iram.start".into(), t.memory.iram.start.clone()),
        ("memory.iram.size".into(), t.memory.iram.size.clone()),
        ("memory.xram.start".into(), t.memory.xram.start.clone()),
        ("memory.xram.size".into(), t.memory.xram.size.clone()),
    ];

    if let Some(cat) = category {
        let prefix = format!("{cat}.");
        pairs.retain(|(k, _)| k.starts_with(&prefix));
    }

    output::display(&OutputValue::KeyValue(pairs), format);
}

fn cmd_build(
    path_str: &str,
    target: &Option<String>,
    command: builder::BuildCommand,
    cfg: &crate::config::EmbConfig,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let path = PathBuf::from(path_str);

    // For workspace files, resolve the sub-project first
    let project_path = if is_workspace_file(&path) {
        let (resolved, _) = resolve_project_path(path_str, &None)?;
        resolved
    } else if is_project_file(&path) {
        path
    } else {
        anyhow::bail!(
            "unsupported file type: {}. Expected .uvprojx or .uvmpw",
            path.display()
        );
    };

    let uv4_path = crate::config::resolve_uv4(cfg).ok();
    let result = builder::build(&project_path, target, command, uv4_path)?;

    let mut pairs = vec![
        ("Success".into(), if result.success { "yes" } else { "no" }.into()),
        ("Errors".into(), result.errors.to_string()),
        ("Warnings".into(), result.warnings.to_string()),
        ("Build Time".into(), result.build_time.clone()),
        ("Output".into(), result.output_file.clone()),
    ];

    if let Some(ps) = &result.program_size {
        pairs.push(("Code Size".into(), format!(
            "Code={} RO-data={} RW-data={} ZI-data={}",
            ps.code, ps.ro_data, ps.rw_data, ps.zi_data
        )));
    }

    output::display(&OutputValue::KeyValue(pairs), format);
    Ok(())
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Resolve path+project to an actual .uvprojx path (handles both direct and workspace).
fn resolve_path(path_str: &str, project: &Option<String>) -> anyhow::Result<PathBuf> {
    let path = PathBuf::from(path_str);
    if is_workspace_file(&path) {
        let (resolved, _) = resolve_project_path(path_str, project)?;
        Ok(resolved)
    } else if is_project_file(&path) {
        Ok(path)
    } else {
        anyhow::bail!(
            "unsupported file type: {}. Expected .uvprojx or .uvmpw",
            path.display()
        )
    }
}

pub fn handle(_cli: &Cli, keil: &super::KeilCommands, cfg: &crate::config::EmbConfig, format: OutputFormat) -> anyhow::Result<()> {
    match keil {
        super::KeilCommands::Info { path, target, project } => {
            cmd_info(path, target, project, format)
        }
        super::KeilCommands::Config { path, target, project, category } => {
            let resolved = resolve_path(path, project)?;
            let proj = load_project(&resolved)?;
            let t = resolve_target(&proj, &Some(target.clone()))?;
            display_config(t, category, format);
            Ok(())
        }
        super::KeilCommands::ConfigSet { path, target, project, key, value } => {
            let resolved = resolve_path(path, project)?;
            editor::config_set(&resolved, target, key, value)?;
            output::display(&OutputValue::Message("ok".into()), format);
            Ok(())
        }
        super::KeilCommands::Defines { path, target, project } => {
            let resolved = resolve_path(path, project)?;
            let proj = load_project(&resolved)?;
            let t = resolve_target(&proj, &Some(target.clone()))?;
            output::display(&OutputValue::List(t.c_compiler.defines.clone()), format);
            Ok(())
        }
        super::KeilCommands::DefinesAdd { path, target, project, macro_name } => {
            let resolved = resolve_path(path, project)?;
            editor::defines_add(&resolved, target, macro_name)?;
            output::display(&OutputValue::Message("ok".into()), format);
            Ok(())
        }
        super::KeilCommands::DefinesRemove { path, target, project, macro_name } => {
            let resolved = resolve_path(path, project)?;
            editor::defines_remove(&resolved, target, macro_name)?;
            output::display(&OutputValue::Message("ok".into()), format);
            Ok(())
        }
        super::KeilCommands::Includes { path, target, project } => {
            let resolved = resolve_path(path, project)?;
            let proj = load_project(&resolved)?;
            let t = resolve_target(&proj, &Some(target.clone()))?;
            output::display(&OutputValue::List(t.c_compiler.include_paths.clone()), format);
            Ok(())
        }
        super::KeilCommands::IncludesAdd { path, target, project, path_to_add } => {
            let resolved = resolve_path(path, project)?;
            editor::includes_add(&resolved, target, path_to_add)?;
            output::display(&OutputValue::Message("ok".into()), format);
            Ok(())
        }
        super::KeilCommands::IncludesRemove { path, target, project, path_to_remove } => {
            let resolved = resolve_path(path, project)?;
            editor::includes_remove(&resolved, target, path_to_remove)?;
            output::display(&OutputValue::Message("ok".into()), format);
            Ok(())
        }
        super::KeilCommands::Groups { path, target, project } => {
            let resolved = resolve_path(path, project)?;
            let proj = load_project(&resolved)?;
            let t = resolve_target(&proj, &Some(target.clone()))?;
            let headers = vec!["Group".into(), "Files".into()];
            let rows: Vec<Vec<String>> = t.groups.iter()
                .map(|g| vec![g.name.clone(), g.files.len().to_string()])
                .collect();
            output::display(&OutputValue::Table { headers, rows }, format);
            Ok(())
        }
        super::KeilCommands::Files { path, target, project, group } => {
            let resolved = resolve_path(path, project)?;
            let proj = load_project(&resolved)?;
            let t = resolve_target(&proj, &Some(target.clone()))?;
            let headers = vec!["Name".into(), "Type".into(), "Path".into(), "Status".into()];
            let mut rows: Vec<Vec<String>> = Vec::new();
            for g in &t.groups {
                if let Some(filter) = group {
                    if g.name != *filter { continue; }
                }
                for f in &g.files {
                    rows.push(vec![
                        f.name.clone(),
                        file_type_str(f.file_type).into(),
                        f.path.clone(),
                        if f.included_in_build { "included" } else { "excluded" }.into(),
                    ]);
                }
            }
            output::display(&OutputValue::Table { headers, rows }, format);
            Ok(())
        }
        super::KeilCommands::GroupAdd { path, target, project, name } => {
            let resolved = resolve_path(path, project)?;
            editor::group_add(&resolved, target, name)?;
            output::display(&OutputValue::Message("ok".into()), format);
            Ok(())
        }
        super::KeilCommands::GroupRemove { path, target, project, name } => {
            let resolved = resolve_path(path, project)?;
            editor::group_remove(&resolved, target, name)?;
            output::display(&OutputValue::Message("ok".into()), format);
            Ok(())
        }
        super::KeilCommands::GroupRename { path, target, project, old, new } => {
            let resolved = resolve_path(path, project)?;
            editor::group_rename(&resolved, target, old, new)?;
            output::display(&OutputValue::Message("ok".into()), format);
            Ok(())
        }
        super::KeilCommands::FileAdd { path, target, project, group, filepath } => {
            let resolved = resolve_path(path, project)?;
            editor::file_add(&resolved, target, group, filepath)?;
            output::display(&OutputValue::Message("ok".into()), format);
            Ok(())
        }
        super::KeilCommands::FileRemove { path, target, project, group, filename } => {
            let resolved = resolve_path(path, project)?;
            editor::file_remove(&resolved, target, group, filename)?;
            output::display(&OutputValue::Message("ok".into()), format);
            Ok(())
        }
        super::KeilCommands::FileExclude { path, target, project, group, filename } => {
            let resolved = resolve_path(path, project)?;
            editor::file_exclude(&resolved, target, group, filename)?;
            output::display(&OutputValue::Message("ok".into()), format);
            Ok(())
        }
        super::KeilCommands::FileInclude { path, target, project, group, filename } => {
            let resolved = resolve_path(path, project)?;
            editor::file_include(&resolved, target, group, filename)?;
            output::display(&OutputValue::Message("ok".into()), format);
            Ok(())
        }
        super::KeilCommands::Build { path, target } => {
            cmd_build(path, target, builder::BuildCommand::Build, cfg, format)
        }
        super::KeilCommands::Rebuild { path, target } => {
            cmd_build(path, target, builder::BuildCommand::Rebuild, cfg, format)
        }
        super::KeilCommands::Clean { path, target } => {
            cmd_build(path, target, builder::BuildCommand::Clean, cfg, format)
        }
        super::KeilCommands::Flash { path, target } => {
            cmd_build(path, target, builder::BuildCommand::Flash, cfg, format)
        }
    }
}
