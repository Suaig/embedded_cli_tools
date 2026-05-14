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

/// Optimization level to label.
fn optim_str(level: u8) -> String {
    match level {
        0 => "O0 (none)".into(),
        1 => "O1".into(),
        2 => "O2".into(),
        3 => "O3 (max)".into(),
        4 => "Os (size)".into(),
        _ => format!("Unknown({level})"),
    }
}

/// Warning level to label.
fn wlevel_str(level: u8) -> String {
    match level {
        0 => "None".into(),
        1 => "Low".into(),
        2 => "Medium".into(),
        3 => "High".into(),
        _ => format!("Unknown({level})"),
    }
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
                ("Compiler Optim".into(), optim_str(t.c_compiler.optimization)),
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

fn cmd_config(
    path_str: &str,
    target_name: &str,
    category: &Option<String>,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let (resolved, _) = resolve_project_path(path_str, &None)?;
    let proj = load_project(&resolved)?;
    let t = resolve_target(&proj, &Some(target_name.to_string()))?;

    match category {
        None => {
            // Overview of all categories
            let total_files: usize = t.groups.iter().map(|g| g.files.len()).sum();
            let pairs = vec![
                ("Device".into(), t.device.name.clone()),
                ("Output".into(), format!("{} ({})", t.output.name, t.output.directory)),
                ("C Compiler".into(), format!("optim={}, defines={}, includes={}",
                    optim_str(t.c_compiler.optimization),
                    t.c_compiler.defines.len(),
                    t.c_compiler.include_paths.len())),
                ("Assembler".into(), format!("defines={}, includes={}",
                    t.assembler.defines.len(),
                    t.assembler.include_paths.len())),
                ("Linker".into(), format!("scatter={}, libs={}",
                    t.linker.scatter_file,
                    t.linker.libs.len())),
                ("Memory".into(), format!("IROM={} +{}, IRAM={} +{}, XRAM={} +{}",
                    t.memory.irom.start, t.memory.irom.size,
                    t.memory.iram.start, t.memory.iram.size,
                    t.memory.xram.start, t.memory.xram.size)),
                ("Groups".into(), t.groups.len().to_string()),
                ("Files".into(), total_files.to_string()),
            ];
            output::display(&OutputValue::KeyValue(pairs), format);
        }
        Some(cat) => match cat.as_str() {
            "device" => {
                let pairs = vec![
                    ("Name".into(), t.device.name.clone()),
                    ("Vendor".into(), t.device.vendor.clone()),
                    ("Pack ID".into(), t.device.pack_id.clone()),
                    ("CPU".into(), t.device.cpu.clone()),
                    ("SVD File".into(), t.device.svd_file.clone()),
                ];
                output::display(&OutputValue::KeyValue(pairs), format);
            }
            "output" => {
                let pairs = vec![
                    ("Name".into(), t.output.name.clone()),
                    ("Directory".into(), t.output.directory.clone()),
                    ("Create Executable".into(), if t.output.create_executable { "yes" } else { "no" }.into()),
                    ("Create Hex".into(), if t.output.create_hex { "yes" } else { "no" }.into()),
                    ("Debug Information".into(), if t.output.debug_information { "yes" } else { "no" }.into()),
                    ("Browse Information".into(), if t.output.browse_information { "yes" } else { "no" }.into()),
                ];
                output::display(&OutputValue::KeyValue(pairs), format);
            }
            "ccompiler" => {
                let pairs = vec![
                    ("Optimization".into(), optim_str(t.c_compiler.optimization)),
                    ("Optimize Time".into(), if t.c_compiler.optimize_time { "yes" } else { "no" }.into()),
                    ("C99".into(), if t.c_compiler.c99 { "yes" } else { "no" }.into()),
                    ("GNU Extensions".into(), if t.c_compiler.gnu { "yes" } else { "no" }.into()),
                    ("Warning Level".into(), wlevel_str(t.c_compiler.warning_level)),
                    ("One ELF Section".into(), if t.c_compiler.one_elf { "yes" } else { "no" }.into()),
                    ("Strict".into(), if t.c_compiler.strict { "yes" } else { "no" }.into()),
                    ("Language".into(), t.c_compiler.lang.to_string()),
                    ("Language Profile".into(), t.c_compiler.lang_profile.to_string()),
                    ("Short Enums".into(), if t.c_compiler.short_enums { "yes" } else { "no" }.into()),
                    ("Misc Controls".into(), t.c_compiler.misc_controls.clone()),
                ];
                output::display(&OutputValue::KeyValue(pairs), format);
            }
            "assembler" => {
                let pairs = vec![
                    ("Defines".into(), t.assembler.defines.join(", ")),
                    ("Include Paths".into(), t.assembler.include_paths.join("; ")),
                    ("Misc Controls".into(), t.assembler.misc_controls.clone()),
                ];
                output::display(&OutputValue::KeyValue(pairs), format);
            }
            "linker" => {
                let pairs = vec![
                    ("Scatter File".into(), t.linker.scatter_file.clone()),
                    ("Libraries".into(), t.linker.libs.join("; ")),
                    ("Lib Paths".into(), t.linker.lib_paths.join("; ")),
                    ("Misc".into(), t.linker.misc.clone()),
                ];
                output::display(&OutputValue::KeyValue(pairs), format);
            }
            "memory" => {
                let pairs = vec![
                    ("IROM Start".into(), t.memory.irom.start.clone()),
                    ("IROM Size".into(), t.memory.irom.size.clone()),
                    ("IRAM Start".into(), t.memory.iram.start.clone()),
                    ("IRAM Size".into(), t.memory.iram.size.clone()),
                    ("XRAM Start".into(), t.memory.xram.start.clone()),
                    ("XRAM Size".into(), t.memory.xram.size.clone()),
                ];
                output::display(&OutputValue::KeyValue(pairs), format);
            }
            _ => {
                anyhow::bail!(
                    "unknown config category: '{}'. Valid: device, output, ccompiler, assembler, linker, memory",
                    cat
                );
            }
        },
    }
    Ok(())
}

fn cmd_defines(
    path_str: &str,
    target_name: &str,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let (resolved, _) = resolve_project_path(path_str, &None)?;
    let proj = load_project(&resolved)?;
    let t = resolve_target(&proj, &Some(target_name.to_string()))?;

    output::display(&OutputValue::List(t.c_compiler.defines.clone()), format);
    Ok(())
}

fn cmd_includes(
    path_str: &str,
    target_name: &str,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let (resolved, _) = resolve_project_path(path_str, &None)?;
    let proj = load_project(&resolved)?;
    let t = resolve_target(&proj, &Some(target_name.to_string()))?;

    output::display(&OutputValue::List(t.c_compiler.include_paths.clone()), format);
    Ok(())
}

fn cmd_groups(
    path_str: &str,
    target_name: &str,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let (resolved, _) = resolve_project_path(path_str, &None)?;
    let proj = load_project(&resolved)?;
    let t = resolve_target(&proj, &Some(target_name.to_string()))?;

    let headers = vec!["Group".into(), "Files".into()];
    let rows: Vec<Vec<String>> = t
        .groups
        .iter()
        .map(|g| vec![g.name.clone(), g.files.len().to_string()])
        .collect();
    output::display(&OutputValue::Table { headers, rows }, format);
    Ok(())
}

fn cmd_files(
    path_str: &str,
    target_name: &str,
    group_filter: &Option<String>,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let (resolved, _) = resolve_project_path(path_str, &None)?;
    let proj = load_project(&resolved)?;
    let t = resolve_target(&proj, &Some(target_name.to_string()))?;

    let headers = vec![
        "Name".into(),
        "Type".into(),
        "Path".into(),
        "Status".into(),
    ];
    let mut rows: Vec<Vec<String>> = Vec::new();

    for g in &t.groups {
        if let Some(filter) = group_filter {
            if g.name != *filter {
                continue;
            }
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

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn handle(_cli: &Cli, keil: &super::KeilCommands, format: OutputFormat) -> anyhow::Result<()> {
    match keil {
        super::KeilCommands::Info { path, target, project } => {
            cmd_info(path, target, project, format)
        }
        super::KeilCommands::Config { path, target, category } => {
            cmd_config(path, target, category, format)
        }
        super::KeilCommands::ConfigSet { .. } => {
            output::not_implemented("keil config set", format);
            Ok(())
        }
        super::KeilCommands::Defines { path, target } => {
            cmd_defines(path, target, format)
        }
        super::KeilCommands::DefinesAdd { .. } => {
            output::not_implemented("keil defines add", format);
            Ok(())
        }
        super::KeilCommands::DefinesRemove { .. } => {
            output::not_implemented("keil defines remove", format);
            Ok(())
        }
        super::KeilCommands::Includes { path, target } => {
            cmd_includes(path, target, format)
        }
        super::KeilCommands::IncludesAdd { .. } => {
            output::not_implemented("keil includes add", format);
            Ok(())
        }
        super::KeilCommands::IncludesRemove { .. } => {
            output::not_implemented("keil includes remove", format);
            Ok(())
        }
        super::KeilCommands::Groups { path, target } => {
            cmd_groups(path, target, format)
        }
        super::KeilCommands::Files { path, target, group } => {
            cmd_files(path, target, group, format)
        }
        super::KeilCommands::GroupAdd { .. } => {
            output::not_implemented("keil group add", format);
            Ok(())
        }
        super::KeilCommands::GroupRemove { .. } => {
            output::not_implemented("keil group remove", format);
            Ok(())
        }
        super::KeilCommands::GroupRename { .. } => {
            output::not_implemented("keil group rename", format);
            Ok(())
        }
        super::KeilCommands::FileAdd { .. } => {
            output::not_implemented("keil file add", format);
            Ok(())
        }
        super::KeilCommands::FileRemove { .. } => {
            output::not_implemented("keil file remove", format);
            Ok(())
        }
        super::KeilCommands::FileExclude { .. } => {
            output::not_implemented("keil file exclude", format);
            Ok(())
        }
        super::KeilCommands::FileInclude { .. } => {
            output::not_implemented("keil file include", format);
            Ok(())
        }
        super::KeilCommands::Build { .. } => {
            output::not_implemented("keil build", format);
            Ok(())
        }
        super::KeilCommands::Rebuild { .. } => {
            output::not_implemented("keil rebuild", format);
            Ok(())
        }
        super::KeilCommands::Clean { .. } => {
            output::not_implemented("keil clean", format);
            Ok(())
        }
        super::KeilCommands::Flash { .. } => {
            output::not_implemented("keil flash", format);
            Ok(())
        }
    }
}
