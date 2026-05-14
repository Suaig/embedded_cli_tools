pub mod parser;
pub mod editor;
pub mod builder;

use crate::output::{self, OutputFormat};

use super::Cli;

pub fn handle(_cli: &Cli, keil: &super::KeilCommands, format: OutputFormat) -> anyhow::Result<()> {
    match keil {
        super::KeilCommands::Info { path, target, project } => {
            let _ = (path, target, project);
            output::not_implemented("keil info", format);
        }
        super::KeilCommands::Config { path, target, category } => {
            let _ = (path, target, category);
            output::not_implemented("keil config", format);
        }
        super::KeilCommands::ConfigSet { path, target, key, value } => {
            let _ = (path, target, key, value);
            output::not_implemented("keil config set", format);
        }
        super::KeilCommands::Defines { path, target } => {
            let _ = (path, target);
            output::not_implemented("keil defines", format);
        }
        super::KeilCommands::DefinesAdd { path, target, macro_name } => {
            let _ = (path, target, macro_name);
            output::not_implemented("keil defines add", format);
        }
        super::KeilCommands::DefinesRemove { path, target, macro_name } => {
            let _ = (path, target, macro_name);
            output::not_implemented("keil defines remove", format);
        }
        super::KeilCommands::Includes { path, target } => {
            let _ = (path, target);
            output::not_implemented("keil includes", format);
        }
        super::KeilCommands::IncludesAdd { path, target, path_to_add } => {
            let _ = (path, target, path_to_add);
            output::not_implemented("keil includes add", format);
        }
        super::KeilCommands::IncludesRemove { path, target, path_to_remove } => {
            let _ = (path, target, path_to_remove);
            output::not_implemented("keil includes remove", format);
        }
        super::KeilCommands::Groups { path, target } => {
            let _ = (path, target);
            output::not_implemented("keil groups", format);
        }
        super::KeilCommands::Files { path, target, group } => {
            let _ = (path, target, group);
            output::not_implemented("keil files", format);
        }
        super::KeilCommands::GroupAdd { path, target, name } => {
            let _ = (path, target, name);
            output::not_implemented("keil group add", format);
        }
        super::KeilCommands::GroupRemove { path, target, name } => {
            let _ = (path, target, name);
            output::not_implemented("keil group remove", format);
        }
        super::KeilCommands::GroupRename { path, target, old, new } => {
            let _ = (path, target, old, new);
            output::not_implemented("keil group rename", format);
        }
        super::KeilCommands::FileAdd { path, target, group, filepath } => {
            let _ = (path, target, group, filepath);
            output::not_implemented("keil file add", format);
        }
        super::KeilCommands::FileRemove { path, target, group, filename } => {
            let _ = (path, target, group, filename);
            output::not_implemented("keil file remove", format);
        }
        super::KeilCommands::FileExclude { path, target, group, filename } => {
            let _ = (path, target, group, filename);
            output::not_implemented("keil file exclude", format);
        }
        super::KeilCommands::FileInclude { path, target, group, filename } => {
            let _ = (path, target, group, filename);
            output::not_implemented("keil file include", format);
        }
        super::KeilCommands::Build { path, target } => {
            let _ = (path, target);
            output::not_implemented("keil build", format);
        }
        super::KeilCommands::Rebuild { path, target } => {
            let _ = (path, target);
            output::not_implemented("keil rebuild", format);
        }
        super::KeilCommands::Clean { path, target } => {
            let _ = (path, target);
            output::not_implemented("keil clean", format);
        }
        super::KeilCommands::Flash { path, target } => {
            let _ = (path, target);
            output::not_implemented("keil flash", format);
        }
    }
    Ok(())
}
