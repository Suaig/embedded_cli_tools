# Embedded CLI Tools (emb) - Design Spec

## 1. Project Overview

A Rust-based CLI tool named `emb` that provides comprehensive embedded development capabilities for AI assistants (primarily Claude Code). The tool enables AI assistants to parse, view, edit Keil projects and STM32CubeMX IOC files, compile/flash firmware, manage serial ports, and debug — all through token-efficient CLI commands.

### Goals

- **Token efficiency**: AI assistants can query and modify embedded projects without reading entire file contents into context
- **Structured access**: Parse binary/opaque formats (XML, Properties) into navigable, hierarchical views
- **Full CRUD**: Not just viewing — complete create/read/update/delete on all project elements
- **Closed-loop workflow**: Edit code → compile → flash → monitor serial → debug, all from CLI
- **Cross-platform design, Windows priority**: Keil only runs on Windows, but serial/IOC tools should work everywhere

### Non-Goals (Phase 1)

- MCP Server integration (may add later)
- TUI/interactive mode
- IDE plugins
- Automated peripheral initialization code generation

---

## 2. Architecture

### 2.1 Single Binary with Subcommands

```
emb <module> <subcommand> [options]
```

Modules: `keil`, `ioc`, `serial`, `debug`

### 2.2 Output Format Strategy

All commands support three output modes:

| Mode | Flag | Description |
|------|------|-------------|
| Human-friendly | (default) | ASCII table/alignment, like `ls`/`df` style |
| AI-optimized | `--ai` | Minimal tokens, compact format AI can parse |
| JSON | `--json` | Full structured JSON output |

### 2.3 Project Structure (Rust)

```
emb/
├── Cargo.toml
├── src/
│   ├── main.rs              # CLI entry, clap argument parsing
│   ├── keil/
│   │   ├── mod.rs
│   │   ├── parser.rs        # .uvprojx / .uvmpx XML parsing
│   │   ├── editor.rs        # XML modification (add/remove/set)
│   │   └── builder.rs       # UV4.exe CLI invocation
│   ├── ioc/
│   │   ├── mod.rs
│   │   ├── parser.rs        # .ioc properties parsing
│   │   ├── editor.rs        # Key-value modification
│   │   └── generator.rs     # STM32CubeMX CLI invocation
│   ├── serial/
│   │   ├── mod.rs
│   │   ├── port.rs          # Serial port scan/open/config
│   │   ├── daemon.rs        # Long-running daemon manager
│   │   └── protocol.rs      # ASCII/HEX encoding
│   ├── debug/
│   │   ├── mod.rs
│   │   ├── openocd.rs       # OpenOCD interface (stub)
│   │   └── keil_debug.rs    # Keil debug interface (stub)
│   └── output/
│       ├── mod.rs
│       ├── human.rs         # Human-friendly formatter
│       ├── ai.rs            # AI-optimized formatter
│       └── json.rs          # JSON formatter
```

---

## 3. Keil Module

### 3.1 File Formats

#### .uvprojx (Single Project)
Standard XML with schema `project_projx.xsd`. Key sections per `<Target>`:

```
<Targets>
  <Target>
    <TargetName>...</TargetName>
    <TargetOption>
      <TargetCommonOption>     → Device, Output, Flash, Memory (Cpu string)
      <CommonProperty>         → IncludeInBuild (target-level)
      <DllOption>              → Debugger DLLs
      <Utilities>              → Flash utilities
      <TargetArmAds>
        <ArmAdsMisc>           → Memory layout (OnChipMemories)
        <Cads>                 → C compiler: Optim, C99, GNU, wLevel, VariousControls (Define, IncludePath)
        <Aads>                 → Assembler: VariousControls
        <LDads>                → Linker: ScatterFile, Libs, Misc
      </TargetArmAds>
    </TargetOption>
    <Groups>
      <Group>
        <GroupName>...</GroupName>
        <Files>
          <File>
            <FileName>...</FileName>
            <FileType>...</FileType>       <!-- 1=.c, 2=.s, 5=.h -->
            <FilePath>...</FilePath>
            <IncludeInBuild>0</IncludeInBuild>  <!-- Present when excluded -->
          </File>
        </Files>
      </Group>
    </Groups>
  </Target>
</Targets>
```

#### .uvmpw (Multi-Project Workspace)
Simple XML referencing multiple `.uvprojx` files:

```xml
<ProjectWorkspace>
  <project>
    <PathAndName>.\Boot\Test_Boot.uvprojx</PathAndName>
    <NodeIsActive>1</NodeIsActive>
    <NodeIsCheckedInBatchBuild>1</NodeIsCheckedInBatchBuild>
  </project>
  <project>
    <PathAndName>.\Appli\Test_Appli.uvprojx</PathAndName>
    <NodeIsCheckedInBatchBuild>1</NodeIsCheckedInBatchBuild>
  </project>
</ProjectWorkspace>
```

### 3.2 Commands

#### Info / Overview

```
emb keil info <path>
```
- `.uvprojx`: lists all Targets with device, toolchain, active status
- `.uvmpw`: lists all sub-projects with active/batch-build status
- With `-t <target>`: detailed config summary for that target
- With `-p <project>` (uvmpw only): scope to a sub-project

#### Config (Read/Write Target Settings)

```
emb keil config <path> -t <target> [category]
```

Categories:

| Category | Maps to XML | Key settings |
|----------|-------------|-------------|
| `device` | TargetCommonOption | Device, Vendor, PackID, Cpu |
| `output` | TargetCommonOption | OutputName, OutputDirectory, CreateHexFile, DebugInformation, BrowseInformation |
| `ccompiler` | Cads | Optim (0-3), oTime, uC99, uGnu, wLevel (0-3), v6Lang, v6LangP, vShortEn |
| `assembler` | Aads | interw, thumb, ClangAsOpt, VariousControls |
| `linker` | LDads | ScatterFile, IncludeLibs, IncludeLibsPath, Misc |
| `memory` | ArmAdsMisc → OnChipMemories | IRAM, IROM, XRAM (StartAddress, Size) |

No category = overview of all.

```
emb keil config set <path> -t <target> <key> <value>
```

Key format uses dot-notation mapping to XML structure:
- `ccompiler.optim` → `<Cads><Optim>`
- `ccompiler.c99` → `<Cads><uC99>`
- `ccompiler.gnu` → `<Cads><uGnu>`
- `ccompiler.wlevel` → `<Cads><wLevel>`
- `output.hex` → `<TargetCommonOption><CreateHexFile>`
- `output.name` → `<TargetCommonOption><OutputName>`
- `output.debug_info` → `<TargetCommonOption><DebugInformation>`
- `device.name` → `<TargetCommonOption><Device>`
- `linker.scatter` → `<LDads><ScatterFile>`
- `memory.irom.start` → `<OnChipMemories><IROM><StartAddress>`
- `memory.irom.size` → `<OnChipMemories><IROM><Size>`

#### Defines

```
emb keil defines <path> -t <target>             # List all defines
emb keil defines add <path> -t <target> <MACRO>  # Add define
emb keil defines remove <path> -t <target> <MACRO>  # Remove define
```

Defines are comma-separated in `<Cads><VariousControls><Define>`.
Examples: `USE_HAL_DRIVER`, `STM32H743xx`, `USE_FULL_LL_DRIVER`

#### Include Paths

```
emb keil includes <path> -t <target>                # List all include paths
emb keil includes add <path> -t <target> <path>      # Add include path
emb keil includes remove <path> -t <target> <path>   # Remove include path
```

Include paths are semicolon-separated in `<Cads><VariousControls><IncludePath>`.

#### Groups & Files

```
emb keil groups <path> -t <target>               # List all groups
emb keil files <path> -t <target>                # All files with compile status
emb keil files <path> -t <target> -g <group>     # Files in specific group

emb keil group add <path> -t <target> <name>
emb keil group remove <path> -t <target> <name>
emb keil group rename <path> -t <target> <old> <new>

emb keil file add <path> -t <target> -g <group> <filepath>
emb keil file remove <path> -t <target> -g <group> <filename>
emb keil file exclude <path> -t <target> -g <group> <filename>
emb keil file include <path> -t <target> -g <group> <filename>
```

File output shows: filename, type, path, compile status (included/excluded).

File type mapping: 1=C source, 2=Assembly, 3=Object, 4=Library, 5=Header, 6=Text

#### Build Operations

```
emb keil build <path> [-t <target>]              # Incremental build
emb keil rebuild <path> [-t <target>]            # Clean + build
emb keil clean <path> [-t <target>]              # Clean artifacts
emb keil flash <path> [-t <target>]              # Flash to target
```

Invokes `UV4.exe` (or `UV4.com` for console output) with appropriate flags:
- build → `UV4 -j0 -b <path> -t <target> -o <logfile>`
- rebuild → `UV4 -j0 -r <path> -t <target> -o <logfile>`
- clean → `UV4 -j0 -c <path> -t <target>`
- flash → `UV4 -j0 -f <path> -t <target>`

`-j0` suppresses GUI, `-sg` disables GUI layout storage.
Output log is captured and parsed for errors/warnings.

For `.uvmpw` files, add `-p <project>` to scope to a sub-project. The actual build still invokes UV4 on the resolved `.uvprojx` path.

#### Multi-Project (-p flag)

All keil commands accept `-p/--project <name>` when operating on `.uvmpw` files:
- `-p` resolves the sub-project's `.uvprojx` path from the workspace
- Subsequent `-t`/operations work on the resolved project
- Without `-p`: operates on the workspace level (info, list sub-projects)

---

## 4. IOC Module

### 4.1 File Format

`.ioc` files use Java Properties format (key=value). Key structure:

```
<Category>.<Property>=<Value>
```

Major categories:

| Category | Content |
|----------|---------|
| `Mcu.*` | MCU model, family, package, pin count, IP list |
| `PA*/PB*/PC*/PD*/PE*/PH*.*` | Pin configs: Mode, Signal, GPIO_Label, GPIO_Speed, GPIO_PuPd, Locked |
| `NVIC.*` | Interrupt priorities and enable states |
| `RCC.*` | Clock tree configuration (PLL, dividers, frequencies) |
| `SPI*/USART*/TIM*/I2C*.*` | Peripheral parameters |
| `VP_*` | Virtual/peripheral internal connections |
| `SH.*` | Shared pin mappings |
| `ProjectManager.*` | Project settings: name, toolchain, heap/stack size, firmware package |
| `PCC.*` | Power consumption calculator data |
| `PinOutPanel.*` | Pin layout info |

### 4.2 Commands

#### Tree-Based Browsing

```
emb ioc info <path>                     # Top-level category list
emb ioc get <path> <prefix>             # Expand by key prefix
```

Behavior:
- `emb ioc info x.ioc`: outputs category list (Mcu, NVIC, RCC, USART1, USART2, SPI3, TIM1, TIM6, ProjectManager, ...)
- `emb ioc get x.ioc RCC`: outputs all `RCC.*` keys and values
- `emb ioc get x.ioc PA13`: outputs all `PA13*` keys (pin config, label, mode, signal)
- `emb ioc get x.ioc ProjectManager`: outputs project settings
- `emb ioc get x.ioc RCC.SYSCLKFreq_VALUE`: outputs single value
- Prefix matching is case-sensitive, matching the IOC file's actual key casing

#### Editing

```
emb ioc set <path> <key> <value>        # Set a specific key
emb ioc rm <path> <key>                 # Remove a key
```

Examples:
- `emb ioc set x.ioc PC13.GPIO_Label LED3`
- `emb ioc set x.ioc USART1.BaudRate 115200`
- `emb ioc set x.ioc ProjectManager.HeapSize 0x400`
- `emb ioc rm x.ioc PC13.GPIO_Label`

#### Code Generation

```
emb ioc generate <path> [--cubemx <cubemx_exe_path>]
```

Invokes STM32CubeMX in quiet mode: `<cubemx_exe> -q <path.ioc>`
- `--cubemx` defaults to checking common install paths:
  - `C:/Program Files/STMicroelectronics/STM32Cube/STM32CubeMX/STM32CubeMX.exe`
  - `C:/ST/STM32CubeMX/STM32CubeMX.exe`
- Returns success/failure and generation log output

---

## 5. Serial Module

### 5.1 Capabilities

- Scan available serial ports
- Short-term: single send/receive with timeout
- Long-term: daemon mode with background monitoring
- ASCII and HEX data modes (`--hex` flag)
- Configurable baud rate, data bits, parity, stop bits
- Multiple simultaneous daemons (each with unique ID)
- Daemon buffer management (read with optional clear)

### 5.2 Commands

#### Port Scanning

```
emb serial scan
```

Lists available COM ports with: port name, manufacturer, VID/PID (if available).

#### Short-Term Operations

```
emb serial send <port> <data> [--hex] [-b <baud>] [-d <databits>] [-p <parity>] [-s <stopbits>]
emb serial recv <port> -t <timeout_ms> [--hex] [-b <baud>] [-d <databits>] [-p <parity>] [-s <stopbits>]
```

- `send`: opens port, sends data, closes port
- `recv`: opens port, waits up to timeout_ms for data, returns received content, closes port
- `--hex`: data is hex-encoded (e.g., `AT\r\n` → `41540D0A`)
- Default baud: 115200, data bits: 8, parity: none, stop bits: 1
- Port opens and closes per operation (no persistent connection)

#### Daemon Operations

```
emb serial daemon start <port> [-b <baud>] [--id <name>] [-d <databits>] [-p <parity>] [-s <stopbits>]
emb serial daemon list
emb serial daemon send <id> <data> [--hex]
emb serial daemon read <id> [-t <timeout_ms>] [--hex] [--clear]
emb serial daemon stop <id>
```

Daemon lifecycle:
1. `start`: spawns background thread, opens serial port, begins continuous reading. Returns daemon ID (auto-generated UUID or user-specified `--id`)
2. Background thread reads incoming data into a ring buffer
3. `send <id>`: writes data to the daemon's serial port
4. `read <id>`: returns buffer contents since last read (or all if no prior read). `--clear` empties buffer after reading. `-t` waits up to timeout for new data if buffer is empty.
5. `list`: shows all running daemons with ID, port, baud, buffer size
6. `stop`: gracefully shuts down daemon, closes port

Daemon communication mechanism:
- Daemon state directory: `%TEMP%/emb/daemons/<id>/` (each daemon gets a directory)
- `buffer.bin`: ring buffer file containing received serial data, with a small header tracking read/write positions
- `meta.toml`: daemon metadata (port, baud, status, PID, start time)
- Send operations: write data directly to the serial port via the daemon's background thread (communicated via a localhost TCP socket on a random port, stored in `meta.toml`)
- Read operations: read from `buffer.bin` respecting the position markers, optionally clear after reading
- This approach avoids OS-specific named pipes while being reliable on Windows/Linux/macOS

### 5.3 Serial Port Crate

Use `serialport` crate (Rust) for cross-platform serial port access.

---

## 6. Debug Module (Stub)

Phase 1 only defines the interface. Implementation deferred.

```
emb debug openocd <args...>              # OpenOCD passthrough (stub)
emb debug keil <path> [-t <target>]     # Keil debug mode (stub)
```

- `keil`: would invoke `UV4 -d <path> -t <target>` to launch Keil in debug mode
- `openocd`: would invoke openocd with provided arguments
- Both return "not yet implemented" in Phase 1

---

## 7. Claude Code Skill

A single skill file that instructs the AI assistant on how to use `emb` commands. The skill covers:

- How to explore a Keil project (info → files → config)
- How to modify a Keil project (add files, change defines, set optimization)
- How to build and flash
- How to browse and edit IOC files
- How to use serial port for debugging
- Output format selection (`--ai` for AI consumption)

The skill does NOT need to understand XML or IOC internals — it just calls `emb` commands and reads the output.

---

## 8. Implementation Phases

### Phase 1: Foundation
- Project scaffolding (Cargo, clap CLI framework)
- Output format infrastructure (human/ai/json)
- Keil parser (uvprojx + uvmpw XML parsing)
- Keil info, config (read), defines, includes, groups, files (read-only)

### Phase 2: Keil Editing
- Keil config set (write)
- Keil defines add/remove
- Keil includes add/remove
- Keil file add/remove/exclude/include
- Keil group add/remove/rename
- Keil XML write-back (preserve formatting where possible)

### Phase 3: Build
- Keil build/rebuild/clean/flash via UV4.exe
- Build log parsing (errors, warnings, success)
- UV4.exe path detection

### Phase 4: IOC
- IOC parser (properties format)
- IOC info, get (tree browsing)
- IOC set/rm (editing)
- IOC generate (STM32CubeMX CLI invocation)

### Phase 5: Serial
- Port scanning
- Short-term send/recv
- Daemon architecture (background thread, shared state)
- Daemon start/list/send/read/stop
- ASCII/HEX mode

### Phase 6: Skill + Debug Stub
- Claude Code skill authoring
- Debug module stubs
- Documentation

### Phase 7: Polish
- Error handling and edge cases
- Integration testing with real projects
- Cross-platform testing (Linux/macOS serial only)

---

## 9. Key Dependencies

| Crate | Purpose |
|-------|---------|
| `clap` | CLI argument parsing with derive macros |
| `roxmltree` or `xmltree` | XML parsing for .uvprojx/.uvmpw |
| `xml-rs` + `xmltree` | XML writing (preserve structure on edit) |
| `serialport` | Cross-platform serial port access |
| `uuid` | Daemon ID generation |
| `anyhow` / `thiserror` | Error handling |
| `tempfile` | Daemon state file management |

---

## 10. Config Key Reference (Keil)

### C Compiler (Cads)

| Key | XML Path | Values |
|-----|----------|--------|
| `ccompiler.optim` | `Cads/Optim` | 0=None, 1=O1, 2=O2, 3=O3 |
| `ccompiler.optimize_time` | `Cads/oTime` | 0=size, 1=time |
| `ccompiler.c99` | `Cads/uC99` | 0=C90, 1=C99 |
| `ccompiler.gnu` | `Cads/uGnu` | 0=off, 1=on |
| `ccompiler.wlevel` | `Cads/wLevel` | 0-3 |
| `ccompiler.one_elf` | `Cads/OneElfS` | 0=off, 1=on |
| `ccompiler.strict` | `Cads/Strict` | 0=off, 1=on |
| `ccompiler.misc` | `Cads/VariousControls/MiscControls` | free text |
| `ccompiler.lang` | `Cads/v6Lang` | AC6 language standard |
| `ccompiler.lang_p` | `Cads/v6LangP` | AC6 language profile |

### Output

| Key | XML Path | Values |
|-----|----------|--------|
| `output.name` | `TargetCommonOption/OutputName` | string |
| `output.directory` | `TargetCommonOption/OutputDirectory` | string |
| `output.hex` | `TargetCommonOption/CreateHexFile` | 0/1 |
| `output.debug_info` | `TargetCommonOption/DebugInformation` | 0/1 |
| `output.browse_info` | `TargetCommonOption/BrowseInformation` | 0/1 |
| `output.executable` | `TargetCommonOption/CreateExecutable` | 0/1 |
| `output.library` | `TargetCommonOption/CreateLib` | 0/1 |

### Device

| Key | XML Path | Values |
|-----|----------|--------|
| `device.name` | `TargetCommonOption/Device` | e.g. STM32H743VITx |
| `device.vendor` | `TargetCommonOption/Vendor` | e.g. STMicroelectronics |
| `device.pack` | `TargetCommonOption/PackID` | e.g. Keil.STM32H7xx_DFP.4.0.0 |
| `device.cpu` | `TargetCommonOption/Cpu` | full CPU string |
| `device.svd` | `TargetCommonOption/SFDFile` | SVD file path |

### Linker

| Key | XML Path | Values |
|-----|----------|--------|
| `linker.scatter` | `LDads/ScatterFile` | scatter file path |
| `linker.misc` | `LDads/Misc` | free text |
| `linker.libs` | `LDads/IncludeLibs` | library paths |
| `linker.lib_paths` | `LDads/IncludeLibsPath` | library search paths |

### Memory

| Key | XML Path | Values |
|-----|----------|--------|
| `memory.irom.start` | `OnChipMemories/IROM/StartAddress` | hex address |
| `memory.irom.size` | `OnChipMemories/IROM/Size` | hex size |
| `memory.iram.start` | `OnChipMemories/IRAM/StartAddress` | hex address |
| `memory.iram.size` | `OnChipMemories/IRAM/Size` | hex size |
| `memory.xram.start` | `OnChipMemories/XRAM/StartAddress` | hex address |
| `memory.xram.size` | `OnChipMemories/XRAM/Size` | hex size |
