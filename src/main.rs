mod keil;
mod ioc;
mod serial;
mod debug;
mod config;
mod output;

use clap::{Parser, Subcommand};
use output::OutputFormat;

/// Embedded development CLI tools
#[derive(Parser)]
#[command(name = "emb", version, about = "Embedded development CLI tools")]
struct Cli {
    /// Output in AI-optimized compact format
    #[arg(long, global = true)]
    ai: bool,

    /// Output in JSON format
    #[arg(long, global = true)]
    json: bool,

    /// Internal: run as serial daemon process (hidden)
    #[arg(long, hide = true)]
    internal_daemon: Option<String>,

    /// Internal: serial port for daemon mode (hidden)
    #[arg(long, hide = true)]
    port: Option<String>,

    /// Internal: baud rate for daemon mode (hidden)
    #[arg(long, hide = true)]
    baud: Option<u32>,

    /// Internal: data bits for daemon mode (hidden)
    #[arg(long, hide = true)]
    data_bits: Option<String>,

    /// Internal: parity for daemon mode (hidden)
    #[arg(long, hide = true)]
    parity: Option<String>,

    /// Internal: stop bits for daemon mode (hidden)
    #[arg(long, hide = true)]
    stop_bits: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Keil UVPROJX project operations
    Keil {
        #[command(subcommand)]
        command: KeilCommands,
    },
    /// STM32CubeMX .ioc file operations
    Ioc {
        #[command(subcommand)]
        command: IocCommands,
    },
    /// Serial port operations
    Serial {
        #[command(subcommand)]
        command: SerialCommands,
    },
    /// Debug operations
    Debug {
        #[command(subcommand)]
        command: DebugCommands,
    },
    /// Configuration management
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },
}

// ---------------------------------------------------------------------------
// Keil subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum KeilCommands {
    /// Show project overview
    Info {
        /// Path to .uvprojx file
        path: String,
        /// Target name (e.g., "Target 1")
        #[arg(short, long)]
        target: Option<String>,
        /// Project name filter
        #[arg(short, long)]
        project: Option<String>,
    },
    /// Show or set target configuration
    Config {
        /// Path to .uvprojx or .uvmpw file
        path: String,
        /// Target name
        #[arg(short, long)]
        target: String,
        /// Project name in workspace (for .uvmpw files)
        #[arg(short, long)]
        project: Option<String>,
        /// Configuration category to show
        category: Option<String>,
    },
    /// Set a configuration key-value pair
    ConfigSet {
        /// Path to .uvprojx or .uvmpw file
        path: String,
        /// Target name
        #[arg(short, long)]
        target: String,
        /// Project name in workspace (for .uvmpw files)
        #[arg(short, long)]
        project: Option<String>,
        /// Configuration key
        key: String,
        /// Configuration value
        value: String,
    },
    /// List preprocessor defines
    Defines {
        /// Path to .uvprojx or .uvmpw file
        path: String,
        /// Target name
        #[arg(short, long)]
        target: String,
        /// Project name in workspace (for .uvmpw files)
        #[arg(short, long)]
        project: Option<String>,
    },
    /// Add a preprocessor define
    DefinesAdd {
        /// Path to .uvprojx or .uvmpw file
        path: String,
        /// Target name
        #[arg(short, long)]
        target: String,
        /// Project name in workspace (for .uvmpw files)
        #[arg(short, long)]
        project: Option<String>,
        /// Macro name to add
        macro_name: String,
    },
    /// Remove a preprocessor define
    DefinesRemove {
        /// Path to .uvprojx or .uvmpw file
        path: String,
        /// Target name
        #[arg(short, long)]
        target: String,
        /// Project name in workspace (for .uvmpw files)
        #[arg(short, long)]
        project: Option<String>,
        /// Macro name to remove
        macro_name: String,
    },
    /// List include paths
    Includes {
        /// Path to .uvprojx or .uvmpw file
        path: String,
        /// Target name
        #[arg(short, long)]
        target: String,
        /// Project name in workspace (for .uvmpw files)
        #[arg(short, long)]
        project: Option<String>,
    },
    /// Add an include path
    IncludesAdd {
        /// Path to .uvprojx or .uvmpw file
        path: String,
        /// Target name
        #[arg(short, long)]
        target: String,
        /// Project name in workspace (for .uvmpw files)
        #[arg(short, long)]
        project: Option<String>,
        /// Include path to add
        path_to_add: String,
    },
    /// Remove an include path
    IncludesRemove {
        /// Path to .uvprojx or .uvmpw file
        path: String,
        /// Target name
        #[arg(short, long)]
        target: String,
        /// Project name in workspace (for .uvmpw files)
        #[arg(short, long)]
        project: Option<String>,
        /// Include path to remove
        path_to_remove: String,
    },
    /// List source groups
    Groups {
        /// Path to .uvprojx or .uvmpw file
        path: String,
        /// Target name
        #[arg(short, long)]
        target: String,
        /// Project name in workspace (for .uvmpw files)
        #[arg(short, long)]
        project: Option<String>,
    },
    /// List files in target
    Files {
        /// Path to .uvprojx or .uvmpw file
        path: String,
        /// Target name
        #[arg(short, long)]
        target: String,
        /// Project name in workspace (for .uvmpw files)
        #[arg(short, long)]
        project: Option<String>,
        /// Filter by group name
        #[arg(short, long)]
        group: Option<String>,
    },
    /// Add a source group
    GroupAdd {
        /// Path to .uvprojx or .uvmpw file
        path: String,
        /// Target name
        #[arg(short, long)]
        target: String,
        /// Project name in workspace (for .uvmpw files)
        #[arg(short, long)]
        project: Option<String>,
        /// Group name
        name: String,
    },
    /// Remove a source group
    GroupRemove {
        /// Path to .uvprojx or .uvmpw file
        path: String,
        /// Target name
        #[arg(short, long)]
        target: String,
        /// Project name in workspace (for .uvmpw files)
        #[arg(short, long)]
        project: Option<String>,
        /// Group name
        name: String,
    },
    /// Rename a source group
    GroupRename {
        /// Path to .uvprojx or .uvmpw file
        path: String,
        /// Target name
        #[arg(short, long)]
        target: String,
        /// Project name in workspace (for .uvmpw files)
        #[arg(short, long)]
        project: Option<String>,
        /// Old group name
        old: String,
        /// New group name
        new: String,
    },
    /// Add a file to a group
    FileAdd {
        /// Path to .uvprojx or .uvmpw file
        path: String,
        /// Target name
        #[arg(short, long)]
        target: String,
        /// Project name in workspace (for .uvmpw files)
        #[arg(short, long)]
        project: Option<String>,
        /// Group name
        #[arg(short, long)]
        group: String,
        /// File path to add
        filepath: String,
    },
    /// Remove a file from a group
    FileRemove {
        /// Path to .uvprojx or .uvmpw file
        path: String,
        /// Target name
        #[arg(short, long)]
        target: String,
        /// Project name in workspace (for .uvmpw files)
        #[arg(short, long)]
        project: Option<String>,
        /// Group name
        #[arg(short, long)]
        group: String,
        /// File name to remove
        filename: String,
    },
    /// Exclude a file from build
    FileExclude {
        /// Path to .uvprojx or .uvmpw file
        path: String,
        /// Target name
        #[arg(short, long)]
        target: String,
        /// Project name in workspace (for .uvmpw files)
        #[arg(short, long)]
        project: Option<String>,
        /// Group name
        #[arg(short, long)]
        group: String,
        /// File name to exclude
        filename: String,
    },
    /// Include a file in build (un-exclude)
    FileInclude {
        /// Path to .uvprojx or .uvmpw file
        path: String,
        /// Target name
        #[arg(short, long)]
        target: String,
        /// Project name in workspace (for .uvmpw files)
        #[arg(short, long)]
        project: Option<String>,
        /// Group name
        #[arg(short, long)]
        group: String,
        /// File name to include
        filename: String,
    },
    /// Build the project
    Build {
        /// Path to .uvprojx file
        path: String,
        /// Target name
        #[arg(short, long)]
        target: Option<String>,
    },
    /// Rebuild the project
    Rebuild {
        /// Path to .uvprojx file
        path: String,
        /// Target name
        #[arg(short, long)]
        target: Option<String>,
    },
    /// Clean build artifacts
    Clean {
        /// Path to .uvprojx file
        path: String,
        /// Target name
        #[arg(short, long)]
        target: Option<String>,
    },
    /// Flash firmware to target
    Flash {
        /// Path to .uvprojx file
        path: String,
        /// Target name
        #[arg(short, long)]
        target: Option<String>,
    },
}

// ---------------------------------------------------------------------------
// IOC subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum IocCommands {
    /// Show IOC file overview
    Info {
        /// Path to .ioc file
        path: String,
    },
    /// Get parameters by prefix
    Get {
        /// Path to .ioc file
        path: String,
        /// Parameter prefix (e.g., "PA5")
        prefix: String,
    },
    /// Set a parameter value
    Set {
        /// Path to .ioc file
        path: String,
        /// Parameter key
        key: String,
        /// Parameter value
        value: String,
    },
    /// Remove a parameter
    Rm {
        /// Path to .ioc file
        path: String,
        /// Parameter key
        key: String,
    },
    /// Generate code from IOC file
    Generate {
        /// Path to .ioc file
        path: String,
        /// Path to STM32CubeMX executable
        #[arg(long)]
        cubemx: Option<String>,
    },
}

// ---------------------------------------------------------------------------
// Serial subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum SerialCommands {
    /// Scan available serial ports
    Scan,
    /// Send data to a serial port
    Send {
        /// Serial port name
        port: String,
        /// Data to send
        data: String,
        /// Interpret data as hex string
        #[arg(long)]
        hex: bool,
        /// Baud rate
        #[arg(short, long, default_value = "115200")]
        baud: u32,
        /// Data bits
        #[arg(short = 'd', long, default_value = "8")]
        data_bits: SerialDataBits,
        /// Parity
        #[arg(short, long, default_value = "none")]
        parity: SerialParity,
        /// Stop bits
        #[arg(short = 's', long, default_value = "1")]
        stop_bits: SerialStopBits,
    },
    /// Receive data from a serial port
    Recv {
        /// Serial port name
        port: String,
        /// Timeout in milliseconds
        #[arg(short, long)]
        timeout: u64,
        /// Display as hex
        #[arg(long)]
        hex: bool,
        /// Baud rate
        #[arg(short, long, default_value = "115200")]
        baud: u32,
        /// Data bits
        #[arg(short = 'd', long, default_value = "8")]
        data_bits: SerialDataBits,
        /// Parity
        #[arg(short, long, default_value = "none")]
        parity: SerialParity,
        /// Stop bits
        #[arg(short = 's', long, default_value = "1")]
        stop_bits: SerialStopBits,
    },
    /// Serial port daemon operations
    Daemon {
        #[command(subcommand)]
        command: DaemonCommands,
    },
}

#[derive(Subcommand)]
enum DaemonCommands {
    /// Start a serial daemon
    Start {
        /// Serial port name
        port: String,
        /// Baud rate
        #[arg(short, long, default_value = "115200")]
        baud: u32,
        /// Daemon instance ID
        #[arg(long)]
        id: Option<String>,
        /// Data bits
        #[arg(short = 'd', long, default_value = "8")]
        data_bits: SerialDataBits,
        /// Parity
        #[arg(short, long, default_value = "none")]
        parity: SerialParity,
        /// Stop bits
        #[arg(short = 's', long, default_value = "1")]
        stop_bits: SerialStopBits,
    },
    /// List running daemons
    List,
    /// Send data through a daemon
    Send {
        /// Daemon instance ID
        id: String,
        /// Data to send
        data: String,
        /// Interpret data as hex string
        #[arg(long)]
        hex: bool,
    },
    /// Read unread data from a daemon
    Read {
        /// Daemon instance ID
        id: String,
        /// Display as hex
        #[arg(long)]
        hex: bool,
    },
    /// View send/receive history
    History {
        /// Daemon instance ID
        id: String,
        /// Max entries to display (default 100, max 1000)
        #[arg(short, long)]
        limit: Option<usize>,
        /// Clear history
        #[arg(long)]
        clear: bool,
    },
    /// Stop a running daemon
    Stop {
        /// Daemon instance ID
        id: String,
    },
}

// Serial parameter value types for CLI parsing
use std::fmt;

#[derive(Debug, Clone, Copy)]
enum SerialDataBits {
    Bits5,
    Bits6,
    Bits7,
    Bits8,
}

impl fmt::Display for SerialDataBits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bits5 => write!(f, "5"),
            Self::Bits6 => write!(f, "6"),
            Self::Bits7 => write!(f, "7"),
            Self::Bits8 => write!(f, "8"),
        }
    }
}

impl std::str::FromStr for SerialDataBits {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "5" => Ok(Self::Bits5),
            "6" => Ok(Self::Bits6),
            "7" => Ok(Self::Bits7),
            "8" => Ok(Self::Bits8),
            _ => Err(format!("invalid data bits: {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum SerialParity {
    None,
    Odd,
    Even,
}

impl fmt::Display for SerialParity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => write!(f, "none"),
            Self::Odd => write!(f, "odd"),
            Self::Even => write!(f, "even"),
        }
    }
}

impl std::str::FromStr for SerialParity {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "none" => Ok(Self::None),
            "odd" => Ok(Self::Odd),
            "even" => Ok(Self::Even),
            _ => Err(format!("invalid parity: {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum SerialStopBits {
    Bits1,
    Bits2,
}

impl fmt::Display for SerialStopBits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bits1 => write!(f, "1"),
            Self::Bits2 => write!(f, "2"),
        }
    }
}

impl std::str::FromStr for SerialStopBits {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "1" => Ok(Self::Bits1),
            "2" => Ok(Self::Bits2),
            _ => Err(format!("invalid stop bits: {s}")),
        }
    }
}

// ---------------------------------------------------------------------------
// Debug subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum DebugCommands {
    /// Run OpenOCD with custom arguments
    Openocd {
        /// Arguments to pass to OpenOCD
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Debug using Keil
    Keil {
        /// Path to .uvprojx file
        path: String,
        /// Target name
        #[arg(short, long)]
        target: Option<String>,
    },
}

// ---------------------------------------------------------------------------
// Config subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum ConfigCommands {
    /// Set a config value
    Set {
        /// Config key (keil_path, cubemx_path)
        key: String,
        /// Config value
        value: String,
        /// Save to user home (global) instead of cwd
        #[arg(long)]
        global: bool,
    },
    /// Remove a config value
    Unset {
        /// Config key
        key: String,
        /// Remove from user home (global) config
        #[arg(long)]
        global: bool,
    },
    /// List current config
    List,
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Internal daemon mode: spawned as a background child process
    if let Some(daemon_id) = &cli.internal_daemon {
        let port_name = cli.port.as_deref().unwrap_or("COM1");
        let baud_rate = cli.baud.unwrap_or(115200);
        let db: u8 = cli.data_bits.as_deref().unwrap_or("8").parse().unwrap_or(8);
        let sb: u8 = cli.stop_bits.as_deref().unwrap_or("1").parse().unwrap_or(1);
        let parity = match cli.parity.as_deref().unwrap_or("none") {
            "odd" => serialport::Parity::Odd,
            "even" => serialport::Parity::Even,
            _ => serialport::Parity::None,
        };
        let data_bits = match db {
            5 => serialport::DataBits::Five,
            6 => serialport::DataBits::Six,
            7 => serialport::DataBits::Seven,
            _ => serialport::DataBits::Eight,
        };
        let stop_bits = match sb {
            2 => serialport::StopBits::Two,
            _ => serialport::StopBits::One,
        };
        let config = serial::port::SerialConfig::new(baud_rate, data_bits, parity, stop_bits);
        return serial::daemon::serve(daemon_id, port_name, baud_rate, &config);
    }

    if cli.ai && cli.json {
        anyhow::bail!("--ai and --json are mutually exclusive");
    }

    let format = if cli.ai {
        OutputFormat::Ai
    } else if cli.json {
        OutputFormat::Json
    } else {
        OutputFormat::Human
    };

    match &cli.command {
        Some(Commands::Keil { command }) => {
            let cfg = config::load();
            keil::handle(&cli, command, &cfg, format)
        }
        Some(Commands::Ioc { command }) => {
            let cfg = config::load();
            ioc::handle(command, &cfg, format)
        }
        Some(Commands::Serial { command }) => serial::handle(command, format),
        Some(Commands::Debug { command }) => debug::handle(command, format),
        Some(Commands::Config { command }) => handle_config(command, format),
        None => {
            anyhow::bail!("no command specified");
        }
    }
}

fn handle_config(cmd: &ConfigCommands, format: OutputFormat) -> anyhow::Result<()> {
    match cmd {
        ConfigCommands::Set { key, value, global } => {
            let path = config::set(key, value, *global)?;
            output::display(
                &output::OutputValue::Message(format!("saved to {}", path.display())),
                format,
            );
        }
        ConfigCommands::Unset { key, global } => {
            let path = config::unset(key, *global)?;
            output::display(
                &output::OutputValue::Message(format!("removed from {}", path.display())),
                format,
            );
        }
        ConfigCommands::List => {
            let cfg = config::load();
            let pairs = vec![
                ("keil_path".into(), cfg.keil_path.unwrap_or_else(|| "-".into())),
                ("cubemx_path".into(), cfg.cubemx_path.unwrap_or_else(|| "-".into())),
            ];
            output::display(&output::OutputValue::KeyValue(pairs), format);
        }
    }
    Ok(())
}
