pub mod parser;
pub mod editor;
pub mod builder;
pub mod map;
pub mod locate;
pub mod probe;

pub use parser::{
    load_project, load_workspace,
    is_workspace_file, is_project_file,
    KeilProject, KeilWorkspace, Target,
};

use std::path::PathBuf;

use anyhow::Context;

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
///
/// uVision stores the 0-based index into the C/C++ "Optimization" dropdown, so
/// the AC6 labels below are the dropdown's own wording.
fn optim_str(level: u8, ac6: bool) -> String {
    if ac6 {
        const OPTIONS: &str =
            "[0=<default> 1=-O0 2=-O1 3=-O2 4=-O3 5=-Ofast 6=-Os balanced 7=-Oz image size 8=-Omax]";
        let label = match level {
            0 => "<default>",
            1 => "-O0",
            2 => "-O1",
            3 => "-O2",
            4 => "-O3",
            5 => "-Ofast",
            6 => "-Os balanced",
            7 => "-Oz image size",
            8 => "-Omax",
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
///
/// uVision stores the 0-based index into the C/C++ "Warnings" dropdown, whose
/// first entry is `<unspecified>` — the same convention as `optim_str`. AC5's
/// dropdown stops at `All Warnings`; AC6 adds two entries.
fn wlevel_str(level: u8, ac6: bool) -> String {
    if ac6 {
        const OPTIONS: &str =
            "[0=<unspecified> 1=No Warnings 2=All Warnings 3=AC5-like Warnings 4=MISRA Compatible]";
        let label = match level {
            0 => "<unspecified>",
            1 => "No Warnings",
            2 => "All Warnings",
            3 => "AC5-like Warnings",
            4 => "MISRA Compatible",
            _ => "?",
        };
        format!("{level} ({label}) {OPTIONS}")
    } else {
        const OPTIONS: &str = "[0=<unspecified> 1=No Warnings 2=All Warnings]";
        let label = match level {
            0 => "<unspecified>",
            1 => "No Warnings",
            2 => "All Warnings",
            _ => "?",
        };
        format!("{level} ({label}) {OPTIONS}")
    }
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

/// Table of a target's active on-chip memory areas (`<OCR_RVCT*>`).
fn memory_areas_table(areas: &[parser::MemoryArea]) -> OutputValue {
    OutputValue::Table {
        headers: vec!["Area".into(), "Type".into(), "Start".into(), "Size".into()],
        rows: areas
            .iter()
            .map(|a| {
                vec![
                    a.tag.clone(),
                    if a.is_rom { "ROM" } else { "RAM" }.into(),
                    a.start.clone(),
                    a.size.clone(),
                ]
            })
            .collect(),
    }
}

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
            let mut sections = vec![OutputValue::KeyValue(pairs)];
            if !t.memory.areas.is_empty() {
                sections.push(memory_areas_table(&t.memory.areas));
            }
            output::display(&OutputValue::Sections(sections), format);
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

/// Top-level category names present in a config key list, in first-seen order.
fn config_categories(pairs: &[(String, String)]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for (key, _) in pairs {
        let category = key.split('.').next().unwrap_or(key).to_string();
        if !seen.contains(&category) {
            seen.push(category);
        }
    }
    seen
}

fn display_config(
    t: &Target,
    category: &Option<String>,
    format: OutputFormat,
) -> anyhow::Result<()> {
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
        ("ccompiler.wlevel".into(), wlevel_str(t.c_compiler.warning_level, t.ac6)),
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
        let available = config_categories(&pairs);
        pairs.retain(|(k, _)| k.starts_with(&prefix));
        if pairs.is_empty() {
            anyhow::bail!(
                "no configuration keys match category '{}' (available: {})",
                cat,
                available.join(", ")
            );
        }
    }

    output::display(&OutputValue::KeyValue(pairs), format);
    Ok(())
}

fn cmd_build(
    path_str: &str,
    target: &Option<String>,
    project: &Option<String>,
    command: builder::BuildCommand,
    opts: builder::BuildOptions,
    cfg: &crate::config::EmbConfig,
    format: OutputFormat,
) -> anyhow::Result<()> {
    // For workspace files this resolves the sub-project selected by `-p`.
    let project_path = resolve_path(path_str, project)?;

    let uv4_path = crate::config::resolve_uv4(cfg).ok();

    // Reject an unknown target here rather than handing it to UV4, which
    // reports it as a silent "Success: no" with no diagnostics.
    //
    // Flash also needs the target name so the probe check can pick the right
    // `<Target>` block out of the .uvoptx.
    let is_flash = matches!(command, builder::BuildCommand::Flash);
    if target.is_some() || is_flash {
        let proj = load_project(&project_path)?;
        let resolved = resolve_target(&proj, target)?;
        if is_flash {
            // Best-effort and warning-only: a project pointed at a probe that
            // is not attached fails inside UV4 with an error naming neither
            // the probe it wanted nor the one that is there.
            probe::warn_if_probe_mismatch(&project_path, &resolved.name);
        }
    }

    let result = builder::build(&project_path, target, command, uv4_path, &opts)?;

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
    ensure_build_succeeded(&result)
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
            display_config(t, category, format)
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
            if let Some(filter) = group {
                if !t.groups.iter().any(|g| g.name == *filter) {
                    let available: Vec<&str> =
                        t.groups.iter().map(|g| g.name.as_str()).collect();
                    anyhow::bail!(
                        "group '{}' not found (available: {})",
                        filter,
                        available.join(", ")
                    );
                }
            }
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
        super::KeilCommands::Build { path, target, project, build } => {
            cmd_build(path, target, project, builder::BuildCommand::Build, build.options(), cfg, format)
        }
        super::KeilCommands::Rebuild { path, target, project, build } => {
            cmd_build(path, target, project, builder::BuildCommand::Rebuild, build.options(), cfg, format)
        }
        super::KeilCommands::Clean { path, target, project, timeout } => {
            let opts = builder::BuildOptions { timeout_secs: *timeout, stream: false };
            cmd_build(path, target, project, builder::BuildCommand::Clean, opts, cfg, format)
        }
        super::KeilCommands::Flash { path, target, project, build } => {
            cmd_build(path, target, project, builder::BuildCommand::Flash, build.options(), cfg, format)
        }
        super::KeilCommands::Map { path, lookup } => {
            match lookup {
                Some(addrs) => {
                    let text = std::fs::read_to_string(path)
                        .with_context(|| format!("read map file: {path}"))?;
                    cmd_map_lookup(&text, addrs, format)?;
                }
                None => cmd_map(&map::parse_map(&PathBuf::from(path))?, format),
            }
            Ok(())
        }
        super::KeilCommands::Locate { path, target, element, project } => {
            let resolved = resolve_path(path, project)?;
            let text = std::fs::read_to_string(&resolved)
                .with_context(|| format!("read {}", resolved.display()))?;
            let snip = match element.as_str() {
                "defines" | "define" => locate::defines(&text, target)?,
                "includes" | "include" | "includepath" => locate::includes(&text, target)?,
                other => locate::config_key(&text, target, other)?,
            };
            let pairs = vec![
                ("Element".into(), element.clone()),
                ("Start Line".into(), snip.start_line.to_string()),
                ("End Line".into(), snip.end_line.to_string()),
            ];
            output::display(
                &OutputValue::Sections(vec![
                    OutputValue::KeyValue(pairs),
                    OutputValue::Message(snip.raw),
                ]),
                format,
            );
            Ok(())
        }
    }
}

/// Render parsed .map info: totals (KeyValue) + execution regions (Table).
fn cmd_map(info: &map::MapInfo, format: OutputFormat) {
    let human = |b: u64| format!("{b} ({:.2}KiB)", b as f64 / 1024.0);
    let pairs: Vec<(String, String)> = vec![
        ("Entry Point".into(), format!("0x{:08X}", info.entry_point)),
        ("Code (inc data)".into(), human(info.totals.code)),
        ("RO Data".into(), human(info.totals.ro_data)),
        ("RW Data".into(), human(info.totals.rw_data)),
        ("ZI Data".into(), human(info.totals.zi_data)),
        ("Total RO".into(), human(info.totals.ro_size)),
        ("Total RW".into(), human(info.totals.rw_size)),
        ("Total ROM".into(), human(info.totals.rom_size)),
    ];
    let headers: Vec<String> = vec![
        "Region".into(),
        "Exec Base".into(),
        "Load Base".into(),
        "Size".into(),
        "Max".into(),
        "Attr".into(),
    ];
    let rows: Vec<Vec<String>> = info
        .regions
        .iter()
        .map(|r| {
            vec![
                r.name.clone(),
                format!("0x{:08X}", r.exec_base),
                format!("0x{:08X}", r.load_base),
                format!("0x{:X} ({})", r.size, r.size),
                format!("0x{:X} ({})", r.max, r.max),
                r.attr.clone(),
            ]
        })
        .collect();

    output::display(
        &OutputValue::Sections(vec![
            OutputValue::KeyValue(pairs),
            OutputValue::Table { headers, rows },
        ]),
        format,
    );
}

/// `keil map --lookup`: translate code addresses into symbols.
///
/// This is the first thing to reach for after a HardFault — the fault handler
/// gives a PC and an LR, and the map is the only place that says what they are.
fn cmd_map_lookup(text: &str, addresses: &str, format: OutputFormat) -> anyhow::Result<()> {
    let symbols = map::parse_map_symbols(text);
    if symbols.is_empty() {
        anyhow::bail!(
            "no symbol table in this map - armlink only writes one when the \
             linker's symbol listing is enabled"
        );
    }

    let headers: Vec<String> = vec![
        "Address".into(),
        "Symbol".into(),
        "Offset".into(),
        "Type".into(),
        "Object(Section)".into(),
    ];
    let mut rows: Vec<Vec<String>> = Vec::new();

    for raw in addresses.split(',') {
        let raw = raw.trim();
        let addr = map::parse_hex(raw)
            .ok_or_else(|| anyhow::anyhow!("not a hex address: '{raw}'"))?;

        rows.push(match map::lookup_symbol(&symbols, addr) {
            Some(hit) => vec![
                format!("0x{addr:08X}"),
                hit.symbol.name.clone(),
                format!("+0x{:X}", hit.offset),
                hit.symbol.kind.clone(),
                hit.symbol.object.clone(),
            ],
            None => vec![
                format!("0x{addr:08X}"),
                "(below every symbol)".into(),
                "-".into(),
                "-".into(),
                "-".into(),
            ],
        });
    }

    output::display(&OutputValue::Table { headers, rows }, format);
    Ok(())
}

/// Turn a finished UV4 run into a command outcome.
///
/// UV4 reports "built but nothing succeeded" as `Success: no` on stdout rather
/// than as a process failure, so a bare `Ok(())` here would make the CLI exit 0
/// on a broken build.
fn ensure_build_succeeded(result: &builder::BuildResult) -> anyhow::Result<()> {
    if result.success {
        return Ok(());
    }

    let mut msg = format!(
        "build failed: {} error(s), {} warning(s)",
        result.errors, result.warnings
    );
    if result.output_file.is_empty() {
        msg.push_str("; UV4 produced no output file");
    }
    let tail: Vec<&str> = result
        .log
        .lines()
        .filter(|l| !l.trim().is_empty())
        .rev()
        .take(5)
        .collect();
    if !tail.is_empty() {
        msg.push_str("\nUV4 log (last lines):");
        for line in tail.iter().rev() {
            msg.push_str("\n  ");
            msg.push_str(line.trim_end());
        }
    }
    anyhow::bail!(msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_result(success: bool, errors: u32) -> builder::BuildResult {
        builder::BuildResult {
            success,
            errors,
            warnings: 0,
            build_time: String::new(),
            program_size: None,
            output_file: String::new(),
            log: String::new(),
        }
    }

    #[test]
    fn successful_build_is_ok() {
        assert!(ensure_build_succeeded(&build_result(true, 0)).is_ok());
    }

    #[test]
    fn failed_build_is_an_error() {
        let err = ensure_build_succeeded(&build_result(false, 3)).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("3 error(s)"),
            "error should report the error count: {msg}"
        );
    }

    /// uVision stores the 0-based index into the C/C++ (AC6) "Warnings"
    /// dropdown, whose first entry is `<unspecified>`.
    #[test]
    fn ac6_warning_levels_match_the_uvision_dropdown() {
        for (value, label) in [
            (0, "<unspecified>"),
            (1, "No Warnings"),
            (2, "All Warnings"),
            (3, "AC5-like Warnings"),
            (4, "MISRA Compatible"),
        ] {
            let s = wlevel_str(value, true);
            assert!(s.starts_with(&format!("{value} ({label})")), "wLevel {value}: {s}");
        }
    }

    /// The AC5 "Warnings" dropdown has three entries.
    #[test]
    fn ac5_warning_levels_match_the_uvision_dropdown() {
        for (value, label) in [
            (0, "<unspecified>"),
            (1, "No Warnings"),
            (2, "All Warnings"),
        ] {
            let s = wlevel_str(value, false);
            assert!(s.starts_with(&format!("{value} ({label})")), "wLevel {value}: {s}");
        }
    }

    /// Same convention for Optimization, whose first entry is `<default>`.
    #[test]
    fn ac6_optimisation_labels_match_the_uvision_dropdown() {
        for (value, label) in [
            (0, "<default>"),
            (1, "-O0"),
            (2, "-O1"),
            (3, "-O2"),
            (4, "-O3"),
            (5, "-Ofast"),
            (6, "-Os balanced"),
            (7, "-Oz image size"),
            (8, "-Omax"),
        ] {
            let s = optim_str(value, true);
            assert!(s.starts_with(&format!("{value} ({label})")), "Optim {value}: {s}");
        }
    }

    /// A value outside the dropdown must still say which value was read.
    #[test]
    fn out_of_range_levels_keep_the_raw_number() {
        assert!(optim_str(9, true).starts_with("9 ("));
        assert!(wlevel_str(9, true).starts_with("9 ("));
    }

    #[test]
    fn config_categories_are_deduplicated_in_order() {
        let pairs = vec![
            ("device.name".to_string(), "x".to_string()),
            ("output.name".to_string(), "x".to_string()),
            ("output.hex".to_string(), "x".to_string()),
            ("ccompiler.optim".to_string(), "x".to_string()),
        ];
        assert_eq!(
            config_categories(&pairs),
            ["device", "output", "ccompiler"]
        );
    }
}
