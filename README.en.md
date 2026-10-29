# emb — Embedded Development CLI Tools

English | [简体中文](README.md)

A command-line toolbox for STM32/ARM embedded development. Parse and edit Keil projects, work with STM32CubeMX IOC files, and manage serial communication — all from the CLI.

## Design Goals

`emb` gives AI assistants (Claude Code, etc.) and human developers token-efficient embedded project management:

- **Token efficient** — when output is captured by an agent, the compact format is used by default; no need to read whole XML/config files
- **Full CRUD** — not just viewing: add, remove, and modify every element of a Keil/IOC project
- **Closed-loop workflow** — edit code → build → flash → serial monitor → debug, all from the CLI
- **Multi-project support** — handle multiple sub-projects in a Keil workspace (`.uvmpw`) with one command

## Features

- **Keil project management** — parse/edit `.uvprojx` (single project) and `.uvmpw` (multi-project workspace)
  - View and modify compiler options (optimization level, C standard, warning level, AC5/AC6 switch)
  - Manage preprocessor defines and include paths
  - Manage source groups; add/remove/exclude files
  - Build (incremental/full/clean) and flash by invoking UV4.exe
  - Builds have a **zero-CPU watchdog**: if UV4 consumes no CPU for 30 consecutive seconds it is treated as stuck in a modal dialog and aborted, with the log tail attached; `--stream` echoes the UV4 log live
  - **`.map` analysis**: ROM/RW/ZI totals + execution regions; `--lookup` translates addresses to symbols (HardFault PC/LR becomes readable)
  - Before flashing, compares the probe configured in `.uvoptx` with the debuggers actually enumerated and warns on a mismatch
- **STM32CubeMX IOC** — parse/edit `.ioc` files
  - Browse peripheral configuration by prefix (RCC, USART, SPI, pins, etc.)
  - Read and write parameter values precisely
  - Invoke the CubeMX CLI to generate code
- **Serial communication** — port scanning, send/receive, background daemon
  - ASCII and HEX modes
  - Long-running daemon mode (multiple instances supported)
  - The daemon supports `--ttl` / `--idle-timeout`, so a forgotten instance still releases the port
  - `serial poke`: no daemon needed; clear buffer → write → wait → read in one call
  - `daemon poke`: the same request/response flow, but through the resident daemon with send/receive history
  - `send` decodes `\r` `\n` `\t` `\0` `\e` `\xHH` by default; `--raw` disables decoding
  - Send/receive history
- **Four output formats** — auto (table on a terminal / compact otherwise) / compact AI / human table / structured JSON

## Installation

### Build from source

```bash
git clone https://github.com/Suaig/embedded_cli_tools.git
cd embedded-cli-tools
cargo build --release
```

The binary is `target/release/emb` (Windows: `target/release/emb.exe`).

Adding `emb` to `PATH` is recommended.

### Prerequisites

- **Rust** 1.70+
- **Keil MDK** (only for build/flash/debug commands)
- **STM32CubeMX** (only for `ioc generate`)

## Quick Start

### Configure tool paths

```bash
# Configure the UV4.exe path (global)
emb config set keil_path "C:\Keil_v5\UV4\UV4.exe" --global

# Configure the STM32CubeMX path
emb config set cubemx_path "C:\ST\STM32CubeMX\STM32CubeMX.exe" --global

# Show current configuration
emb config list
```

### Explore a Keil project

```bash
# Single-project overview
emb keil info project.uvprojx

# Target details
emb --ai keil info project.uvprojx -t "Target 1"

# All compiler configuration (with allowed value ranges)
emb --ai keil config project.uvprojx -t "Target 1"

# Source files
emb keil files project.uvprojx -t "Target 1"

# Multi-project workspace
emb keil info workspace.uvmpw
emb --ai keil config workspace.uvmpw -t "Target 1" -p "SubProject.uvprojx"
```

### Modify compiler options

```bash
# First look at the current value and allowed range
emb --ai keil config project.uvprojx -t "Target 1" ccompiler

# Change the optimization level
emb keil config-set project.uvprojx -t "Target 1" ccompiler.optim 6

# Change the warning level
emb keil config-set project.uvprojx -t "Target 1" ccompiler.wlevel 3

# Enable GNU extensions
emb keil config-set project.uvprojx -t "Target 1" ccompiler.gnu yes
```

### Build and flash

```bash
emb keil build project.uvprojx -t "Target 1"
emb keil build project.uvprojx -t "Target 1" --stream       # Echo the UV4 log live (stderr)
emb keil build project.uvprojx -t "Target 1" --timeout 900  # Wall-clock cap; default 0 = unlimited
emb keil flash project.uvprojx -t "Target 1"

# Analyze a map: totals/regions, or translate addresses to symbols
emb keil map Objects/project.map
emb keil map Objects/project.map --lookup 0x90034FCE,0x90035398
```

### Work with IOC files

```bash
emb ioc info test.ioc                      # List all peripheral categories
emb ioc get test.ioc RCC                   # Show clock configuration
emb ioc get test.ioc PA5                   # Show pin configuration
emb ioc set test.ioc PC13.GPIO_Label LED   # Change a pin label
emb ioc generate test.ioc                  # Invoke CubeMX to generate code
```

### Serial communication

```bash
emb serial scan                            # Scan available ports
emb serial send COM3 "AT\r\n"              # Character mode: \r \n \t \0 \e \xHH are decoded
emb serial send COM3 41540d --hex          # Send as hex
emb serial poke COM3 "version\r" --wait 800 # Write→wait→read in one call; no daemon
emb serial recv COM3 --timeout 3000        # Read; exits non-zero when no data arrives

# Daemon mode (long-running monitoring / send-receive history)
emb serial daemon start COM3 --baud 115200 --id my-device
emb serial daemon start COM3 --id my-device --ttl 600          # Release the port after 10 minutes
emb serial daemon start COM3 --id my-device --idle-timeout 120 # Release after 2 minutes idle
emb serial daemon send my-device "hello\r"                     # Escapes are decoded here too
emb serial daemon poke my-device 7f --wait 50                  # data is always hex
emb serial daemon read my-device --show-hex
emb serial daemon list                                         # Status / Limits columns
emb serial daemon history my-device
emb serial daemon stop my-device
```

## Output Formats

Every command supports four output modes. **The default is auto**: a terminal gets the human table, while pipes / agents / scripts get the compact AI format — an agent gets the cheapest format without passing any flag.

| Flag | Purpose | Example |
|------|---------|---------|
| default (auto) | terminal → human table; non-terminal → compact AI | `emb keil files project.uvprojx -t "Target 1"` |
| `--ai` | force compact AI format, fewest tokens | `emb --ai keil config project.uvprojx -t "Target 1"` |
| `--json` | force structured JSON, for jq / scripts | `emb --json keil info project.uvprojx` |
| `--human` | force the human table (even when captured) | `emb --human keil files project.uvprojx` |

`--ai` / `--json` / `--human` are mutually exclusive. JSON is pretty-printed and is roughly 1.5–4× the size of AI output (`keil files` measured 35.8 KB vs 20.1 KB); use it only when programmatic parsing is needed.

AI output example:

```
device.name:STM32H750XBHx
output.name:signal_project
output.hex:yes
ccompiler.optim:4 (-O3) [0=<default> 1=-O0 2=-O1 3=-O2 4=-O3 5=-Ofast 6=-Os balanced 7=-Oz image size 8=-Omax]
ccompiler.wlevel:3 (AC5-like Warnings) [0=<unspecified> 1=No Warnings 2=All Warnings 3=AC5-like Warnings 4=MISRA Compatible]
```

Tables use `|` as the separator; a `|` inside a cell is escaped as `\|`. Multi-section output (`keil map`, `keil locate`, `keil info -t`, `debug keil`) is separated by a `---` line — blank lines cannot be used as boundaries because raw XML / UV4 logs may contain them.

`--json` emits exactly **one** JSON document per invocation, with a `kind` discriminator so consumers can dispatch on one field instead of guessing the shape from the command:

| kind | Other fields | Commands |
|---|---|---|
| `table` | `headers`, `rows` | `keil info` (no target), `keil groups`, `keil files`, `keil map --lookup`, `ioc info`, `serial scan`, `serial daemon list`, `serial daemon history` |
| `kv` | `entries` (`[key, value]` array) | `keil config`, `keil build`, `ioc get <prefix>`, `config set` / `unset` |
| `list` | `items` | `keil defines`, `keil includes` |
| `message` | `value` | `"ok"` from write commands, `ioc get <exact key>`, the raw snippet from `keil locate`, empty-state messages from `serial` |
| `sections` | `sections` (an array of the above) | `keil map`, `keil locate`, `keil info -t <T>` (KV header + optional memory-area table), `debug keil` (KV header + dump + UV4 log on error) — when one command produces several blocks. `keil map --lookup` is the exception: it answers one question, so it is a single `table` |

```bash
$ emb --json ioc get project.ioc Mcu.Name     # exact hit → message
{ "kind": "message", "value": "STM32H750XBHx" }

$ emb --json ioc get project.ioc GPIO         # prefix hit → kv
{ "kind": "kv", "entries": [["GPIO.groupedBy", "Show All"]] }

$ emb --json keil map Objects/out.map | jq '.sections[] | .kind'
"kv"
"table"
```

## Full Command Reference

### Keil

```
emb keil info <path> [-t <target>] [-p <project>]
emb keil config <path> -t <target> [category] [-p <project>]
emb keil config-set <path> -t <target> <key> <value> [-p <project>]
emb keil defines <path> -t <target> [-p <project>]
emb keil defines-add <path> -t <target> <macro> [-p <project>]
emb keil defines-remove <path> -t <target> <macro> [-p <project>]
emb keil includes <path> -t <target> [-p <project>]
emb keil includes-add <path> -t <target> <path> [-p <project>]
emb keil includes-remove <path> -t <target> <path> [-p <project>]
emb keil groups <path> -t <target> [-p <project>]
emb keil files <path> -t <target> [-g <group>] [-p <project>]
emb keil group-add <path> -t <target> <name> [-p <project>]
emb keil group-remove <path> -t <target> <name> [-p <project>]
emb keil group-rename <path> -t <target> <old> <new> [-p <project>]
emb keil file-add <path> -t <target> -g <group> <filepath> [-p <project>]
emb keil file-remove <path> -t <target> -g <group> <filename> [-p <project>]
emb keil file-exclude <path> -t <target> -g <group> <filename> [-p <project>]
emb keil file-include <path> -t <target> -g <group> <filename> [-p <project>]
emb keil build <path> [-t <target>] [-p <project>] [--timeout <sec>] [--stream] [--all-targets]
emb keil rebuild <path> [-t <target>] [-p <project>] [--timeout <sec>] [--stream] [--all-targets]
emb keil clean <path> [-t <target>] [-p <project>] [--timeout <sec>]
emb keil flash <path> [-t <target>] [-p <project>] [--timeout <sec>] [--stream]
emb keil map <path> [--lookup <addr[,addr...]>]
                                             # Analyze an armlink .map (ROM/RW/ZI + execution regions)
                                             # --lookup translates addresses: symbol + offset + type + object
emb keil locate <path> -t <target> <element> [-p <project>]
                                             # Locate an XML node (start_line / end_line /
                                             # Read Offset / Read Limit + raw snippet)
                                             # element: defines | includes | Optim | ScatterFile | File | ...
```

### IOC

```
emb ioc info <path>
emb ioc get <path> <prefix>
emb ioc set <path> <key> <value>
emb ioc rm <path> <key>
emb ioc generate <path> [--cubemx <path>]
```

### Serial

```
emb serial scan
emb serial send <port> <data> [--hex|--raw] [-b <baud>] [-d <bits>] [-p <parity>] [-s <stop>]
                               # Decodes \r \n \t \0 \e \xHH by default; --raw sends literally
emb serial poke <port> <data> [--wait <ms>] [--hex|--raw] [--show-hex]
                               [-b <baud>] [-d <bits>] [-p <parity>] [-s <stop>]
                               # clear buffer → write → wait → read in one call
emb serial recv <port> --timeout <ms> [--show-hex] [-b <baud>] [-d <bits>] [-p <parity>] [-s <stop>]
emb serial daemon start <port> [-b <baud>] [--id <name>] [-d <bits>] [-p <parity>] [-s <stop>]
                               [--ttl <sec>] [--idle-timeout <sec>]
emb serial daemon list
emb serial daemon send <id> <data> [--hex|--raw]
emb serial daemon poke <id> <hex> [--wait <ms>] [--repeat <n>] [--interval <ms>] [--show-hex]
                               # data is always hex; clear buffer → write → wait → read in one call
emb serial daemon read <id> [--show-hex]
emb serial daemon history <id> [--limit <n>] [--clear]
emb serial daemon stop <id>
```

### Config

```
emb config set <key> <value> [--global]
emb config unset <key> [--global]
emb config list
```

### Debug

```
emb debug openocd [args...]                       # Pass arguments through to OpenOCD (bring your own openocd)
emb debug keil <path> [-t <target>] [options]     # Scripted debugging via UV4 -d
  --regs                    Read CPU registers R0-R3/R12-R15/xPSR
  --read <addr[%size]>      Read memory (4 bytes by default; %N selects the byte count)
  --dump <start,end>        Dump a memory range
  --break <addr|func>       Set a breakpoint
  --run-to <addr|func>      Run to that point, then sample (uses `G ,<sym>`, a temporary breakpoint)
  --step <N> / --pstep <N>  Step N times (step into / step over)
  --ini <file>              Custom .ini (must contain its own EXIT)
  --timeout <sec>           UV4 -d timeout (default 60)
  --no-reset                Skip the initial RESET
  --no-download             Do not download: read the firmware already running on the board (temporarily clears the download flag, restores it afterwards)
```

> `debug keil` uses the debugger configured in the project (`.uvoptx`'s `<pMon>`, e.g. CMSIS-DAP) and runs in **batch mode** (one .ini script dumps results, then exits) — it is not interactive stepping. For live stepping use openocd + gdb. See `skills/keil-dev/references/debug.md` for details.

## Configuration

`emb` uses a TOML config file `.embconfig.toml`, loaded by priority:

1. `~/.embconfig.toml` (user-global, higher priority)
2. `./.embconfig.toml` (current project, lower priority)

Supported keys:

| Key | Purpose |
|-----|---------|
| `keil_path` | Full path to UV4.exe |
| `cubemx_path` | Full path to STM32CubeMX.exe |

When unset, the tool searches common install paths and environment variables (`UV4_PATH`, `KEIL_PATH`, `CUBEMX_PATH`).

## Build and Test

```bash
# Development build
cargo build

# Release build
cargo build --release

# Run tests
cargo test

# Lint
cargo clippy
```

## Project Structure

```
emb/
├── Cargo.toml
├── src/
│   ├── main.rs              # CLI entry point, clap argument parsing
│   ├── config.rs            # Configuration management (.embconfig.toml)
│   ├── keil/
│   │   ├── mod.rs           # Command routing
│   │   ├── parser.rs        # .uvprojx/.uvmpw XML parsing
│   │   ├── editor.rs        # XML modification (CRUD)
│   │   ├── builder.rs       # UV4.exe invocation (zero-CPU watchdog / log streaming)
│   │   ├── map.rs           # .map parsing (totals / execution regions / symbol table + address lookup)
│   │   ├── locate.rs        # XML node location (line numbers + raw snippet)
│   │   └── probe.rs         # Probe pre-check (.uvoptx pMon vs PnP enumeration)
│   ├── ioc/
│   │   ├── mod.rs           # Command routing
│   │   ├── parser.rs        # .ioc Properties parsing
│   │   ├── editor.rs        # Key-value modification
│   │   └── generator.rs     # STM32CubeMX CLI invocation
│   ├── serial/
│   │   ├── mod.rs           # Command routing
│   │   ├── port.rs          # Port scanning/opening/send-receive
│   │   ├── daemon.rs        # Background daemon (TTL / idle timeout / poke)
│   │   ├── protocol.rs      # ASCII/HEX encoding and decoding
│   │   └── spawn.rs         # Detach with a handle whitelist (Windows)
│   ├── debug/
│   │   ├── mod.rs           # Command routing
│   │   ├── openocd.rs       # OpenOCD interface (stub)
│   │   └── keil_debug.rs    # Scripted UV4 -d debugging (.ini generation / uvoptx·uvprojx patching)
│   └── output/
│       ├── mod.rs           # OutputFormat enum
│       ├── human.rs         # Human-friendly format
│       ├── ai.rs            # Compact AI format
│       └── json.rs          # JSON format
├── tests/                   # CLI-level integration tests (drive the real binary)
├── skills/
│   └── keil-dev/            # Claude Code Skill (SKILL.md + references/)
└── docs/
    └── plan/
        └── 测试验收_规格.md  # Test acceptance specification
```

## Tech Stack

- **Rust** 2021 edition
- **clap** 4 — CLI argument parsing
- **roxmltree** / **xmltree** — Keil XML parsing/writing
- **serialport** 4 — cross-platform serial communication
- **serde** / **serde_json** / **toml** — serialization/configuration
- **comfy-table** — terminal table rendering
- **uuid** — daemon ID generation

## Claude Code Skills

The repository ships `skills/keil-dev/` — teaching AI to operate Keil projects efficiently with emb:

- **Task → path decisions**: which emb command to use for viewing / changing values / adding or removing nodes / building / debugging / map analysis
- **locate + Read + Edit workflow**: edit a 3000+ line `.uvprojx` with ~300× fewer tokens, without breaking XML formatting
- **Scripted UV4 -d debugging**: read registers / memory / sample with `--run-to` / single-step dumps, including `--no-download` and batch-mode boundaries
- **Troubleshooting guide**: stuck builds (zero-CPU watchdog), probe mismatches, daemon timeouts and `poke`, `map --lookup` for HardFault addresses
- Pitfalls such as editor formatting warnings, `emb keil locate` offsets, and daemon pipes

It uses progressive disclosure: SKILL.md is a concise decision guide; command details live in `references/` (commands / editing / debug / serial / install) and are read on demand.

**Installation**: see [`skills/keil-dev/references/install.md`](skills/keil-dev/references/install.md), or point your skill installer at `skills/keil-dev/`.

> For interactive single-step debugging, use openocd + gdb (or the `openocd-debug` skill if installed); it complements the batch mode of `emb debug keil`.

## License

MIT
