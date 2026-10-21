pub mod openocd;
pub mod keil_debug;

use std::path::PathBuf;

use anyhow::Context;

use crate::output::{self, OutputFormat, OutputValue};

pub fn handle(
    debug: &super::DebugCommands,
    cfg: &crate::config::EmbConfig,
    format: OutputFormat,
) -> anyhow::Result<()> {
    match debug {
        super::DebugCommands::Openocd { args } => {
            let _ = args;
            let msg = "OpenOCD support not yet implemented. Planned for future release.";
            output::display(&OutputValue::Message(msg.to_string()), format);
        }
        super::DebugCommands::Keil {
            path,
            target,
            read,
            dump,
            regs,
            break_,
            run_to,
            step,
            pstep,
            ini,
            timeout,
            no_reset,
            no_download,
        } => {
            let proj = PathBuf::from(&path);
            let target_name = match target {
                Some(t) => t.clone(),
                None => {
                    let p = crate::keil::parser::load_project(&proj)?;
                    p.targets
                        .first()
                        .map(|t| t.name.clone())
                        .context("no target found in project")?
                }
            };
            // Same pre-flight as `keil flash`: a project pointed at a probe that
            // is not attached fails inside UV4 with an error naming neither side.
            crate::keil::probe::warn_if_probe_mismatch(&proj, &target_name);

            let args = keil_debug::DebugArgs {
                read: read.clone(),
                dump: dump.clone(),
                regs: *regs,
                break_: break_.clone(),
                run_to: run_to.clone(),
                step: *step,
                pstep: *pstep,
                ini: ini.as_ref().map(PathBuf::from),
                timeout: *timeout,
                no_reset: *no_reset,
                no_download: *no_download,
            };
            let res = keil_debug::run(&proj, &target_name, &args, cfg)?;
            let pairs = vec![
                ("Target".into(), target_name.clone()),
                (
                    "Download".into(),
                    if *no_download {
                        "skipped (--no-download)".into()
                    } else {
                        "as configured in the project".into()
                    },
                ),
                ("Exit Code".into(), res.exit_code.to_string()),
                (
                    "Timed Out".into(),
                    if res.timed_out {
                        "yes".into()
                    } else {
                        "no".into()
                    },
                ),
            ];
            // One document even in --json: the header, the debug dump, and (only
            // when something went wrong) UV4's own log.
            let mut sections = vec![OutputValue::KeyValue(pairs)];
            if !res.dump.is_empty() {
                let filtered = keil_debug::filter_dump(&res.dump);
                if !filtered.is_empty() {
                    sections.push(OutputValue::Message(filtered));
                }
            }
            if res.timed_out || res.exit_code != 0 {
                // "Timed out" on its own reads like a tool problem. When the
                // session was waiting for a symbol, say so: the usual cause is
                // that the target never runs that code at all (a bootloader
                // that rejects the image, a breakpoint on the wrong core).
                let waited_for = run_to.clone().or_else(|| break_.clone());
                if res.timed_out {
                    if let Some(sym) = waited_for {
                        sections.push(OutputValue::Message(format!(
                            "note: the target never reached '{sym}' within {}s. \
                             If the board is sitting in a bootloader, that code was \
                             never executed - check that the image the bootloader \
                             starts is actually the one this project built.",
                            timeout
                        )));
                    }
                }
                // "Target DLL has been cancelled" names neither the probe the
                // project wants nor the fact that none is attached at all.
                if let Some(hint) =
                    crate::keil::probe::missing_probe_hint(&proj, &target_name)
                {
                    sections.push(OutputValue::Message(hint));
                }
                if let Some(tail) = keil_debug::log_tail(&res.uv4_log) {
                    sections.push(OutputValue::Message(tail));
                }
            }
            output::display(&OutputValue::Sections(sections), format);
        }
    }
    Ok(())
}
