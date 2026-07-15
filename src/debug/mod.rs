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
            };
            let res = keil_debug::run(&proj, &target_name, &args, cfg)?;
            let pairs = vec![
                ("Target".into(), target_name.clone()),
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
            output::display(&OutputValue::KeyValue(pairs), format);
            if !res.dump.is_empty() {
                let filtered = keil_debug::filter_dump(&res.dump);
                if !filtered.is_empty() {
                    output::display(&OutputValue::Message(filtered), format);
                }
            }
        }
    }
    Ok(())
}
