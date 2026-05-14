pub mod parser;
pub mod editor;
pub mod generator;

use crate::output::{self, OutputFormat};

pub fn handle(ioc: &super::IocCommands, format: OutputFormat) -> anyhow::Result<()> {
    match ioc {
        super::IocCommands::Info { path } => {
            let _ = path;
            output::not_implemented("ioc info", format);
        }
        super::IocCommands::Get { path, prefix } => {
            let _ = (path, prefix);
            output::not_implemented("ioc get", format);
        }
        super::IocCommands::Set { path, key, value } => {
            let _ = (path, key, value);
            output::not_implemented("ioc set", format);
        }
        super::IocCommands::Rm { path, key } => {
            let _ = (path, key);
            output::not_implemented("ioc rm", format);
        }
        super::IocCommands::Generate { path, cubemx } => {
            let _ = (path, cubemx);
            output::not_implemented("ioc generate", format);
        }
    }
    Ok(())
}
