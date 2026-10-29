# emb — Embedded Development CLI Tools

[English](README.en.md) | 简体中文

面向 STM32/ARM 嵌入式开发的命令行工具箱。解析、编辑、构建 Keil 项目，操作 STM32CubeMX IOC 文件，管理串口通信 — 全部通过 CLI 完成。

## 设计目标

`emb` 为 AI 助手（Claude Code 等）和人类开发者提供 token 高效的嵌入式项目管理能力：

- **token 高效** — 输出被 Agent 捕获时默认用紧凑格式，无需读取整个 XML/配置文件
- **完整 CRUD** — 不仅查看，还能增删改 Keil/IOC 项目的所有元素
- **闭环工作流** — 编辑代码 → 编译 → 烧录 → 串口监控 → 调试，全在 CLI
- **多项目支持** — 一条命令处理 Keil workspace（`.uvmpw`）中的多个子项目

## 功能

- **Keil 项目管理** — 解析/编辑 `.uvprojx`（单项目）和 `.uvmpw`（多项目工作区）
  - 查看和修改编译选项（优化级别、C 标准、警告级别、AC5/AC6 切换）
  - 管理预处理器宏定义和头文件搜索路径
  - 管理源文件分组、添加/删除/排除文件
  - 构建（增量/全量/清理）和烧录，调用 UV4.exe
  - 构建带**零 CPU 看门狗**：UV4 连续 30 秒不消耗 CPU 即判定卡在模态对话框并中止，附日志尾；`--stream` 可实时回显 UV4 日志
  - **`.map` 分析**：ROM/RW/ZI 总量 + 执行区域；`--lookup` 把地址翻译成符号（HardFault 的 PC/LR 直接可读）
  - 烧录前比对 `.uvoptx` 里配置的探针与实际枚举到的调试器，不一致先说明
- **STM32CubeMX IOC** — 解析/编辑 `.ioc` 文件
  - 按前缀浏览外设配置（RCC、USART、SPI、引脚等）
  - 精确读写参数值
  - 调用 CubeMX CLI 生成代码
- **串口通信** — 端口扫描、收发数据、后台守护进程
  - ASCII 和 HEX 双模式
  - 长期监控的 daemon 模式（支持多个实例）
  - daemon 可设 `--ttl` / `--idle-timeout`，忘了关闭也会自己释放端口
  - `serial poke`：不启 daemon，一次调用内完成「清缓冲 → 写 → 等 → 读」
  - `daemon poke`：同样的问答流程，但走常驻 daemon 并留下收发历史
  - `send` 默认解码 `\r` `\n` `\t` `\0` `\e` `\xHH`，`--raw` 关掉解码
  - 收发历史记录
- **四种输出格式** — 自动（终端表格 / 非终端紧凑）/ AI 紧凑 / 人类表格 / 结构化 JSON

## 安装

### 从源码构建

```bash
git clone https://github.com/Suaig/embedded_cli_tools.git
cd embedded-cli-tools
cargo build --release
```

编译产物在 `target/release/emb`（Windows: `target/release/emb.exe`）。

建议将 `emb` 添加到 PATH。

### 前置依赖

- **Rust** 1.70+
- **Keil MDK**（仅 build/flash/debug 命令需要）
- **STM32CubeMX**（仅 `ioc generate` 需要）

## 快速开始

### 配置工具路径

```bash
# 配置 UV4.exe 路径（全局生效）
emb config set keil_path "C:\Keil_v5\UV4\UV4.exe" --global

# 配置 STM32CubeMX 路径
emb config set cubemx_path "C:\ST\STM32CubeMX\STM32CubeMX.exe" --global

# 查看当前配置
emb config list
```

### 探索 Keil 项目

```bash
# 单项目概览
emb keil info project.uvprojx

# 查看 Target 详情
emb --ai keil info project.uvprojx -t "Target 1"

# 查看所有编译配置（含可选值范围）
emb --ai keil config project.uvprojx -t "Target 1"

# 查看源文件
emb keil files project.uvprojx -t "Target 1"

# 多项目工作区
emb keil info workspace.uvmpw
emb --ai keil config workspace.uvmpw -t "Target 1" -p "SubProject.uvprojx"
```

### 修改编译选项

```bash
# 先查看当前值和可选范围
emb --ai keil config project.uvprojx -t "Target 1" ccompiler

# 修改优化级别
emb keil config-set project.uvprojx -t "Target 1" ccompiler.optim 6

# 修改警告级别
emb keil config-set project.uvprojx -t "Target 1" ccompiler.wlevel 3

# 启用 GNU 扩展
emb keil config-set project.uvprojx -t "Target 1" ccompiler.gnu yes
```

### 构建和烧录

```bash
emb keil build project.uvprojx -t "Target 1"
emb keil build project.uvprojx -t "Target 1" --stream       # 实时回显 UV4 日志（stderr）
emb keil build project.uvprojx -t "Target 1" --timeout 900  # 墙钟上限，默认 0 = 不限
emb keil flash project.uvprojx -t "Target 1"

# 分析 map：总量/区域，或把地址翻成符号
emb keil map Objects/project.map
emb keil map Objects/project.map --lookup 0x90034FCE,0x90035398
```

### 操作 IOC 文件

```bash
emb ioc info test.ioc                      # 查看所有外设分类
emb ioc get test.ioc RCC                   # 查看时钟配置
emb ioc get test.ioc PA5                   # 查看引脚配置
emb ioc set test.ioc PC13.GPIO_Label LED   # 修改引脚标签
emb ioc generate test.ioc                  # 调用 CubeMX 生成代码
```

### 串口通信

```bash
emb serial scan                            # 扫描可用串口
emb serial send COM3 "AT\r\n"              # 字符模式：\r \n \t \0 \e \xHH 会解码
emb serial send COM3 41540d --hex          # 按 hex 发
emb serial poke COM3 "version\r" --wait 800 # 发→等→读一次完成，不需要 daemon
emb serial recv COM3 --timeout 3000        # 读；超时无数据返回非零

# Daemon 模式（长期监控 / 需要收发历史）
emb serial daemon start COM3 --baud 115200 --id my-device
emb serial daemon start COM3 --id my-device --ttl 600          # 10 分钟后自我释放端口
emb serial daemon start COM3 --id my-device --idle-timeout 120 # 闲置 2 分钟自我释放
emb serial daemon send my-device "hello\r"                     # 同样解码转义
emb serial daemon poke my-device 7f --wait 50                  # data 恒为 hex
emb serial daemon read my-device --show-hex
emb serial daemon list                                         # Status / Limits 两列
emb serial daemon history my-device
emb serial daemon stop my-device
```

## 输出格式

所有命令支持四种输出模式。**默认是自动**：stdout 接终端（人看）用表格，被管道 / Agent / 脚本捕获用 AI 紧凑格式——Agent 不加 flag 就能拿到最省 token 的格式。

| Flag | 用途 | 示例 |
|------|------|------|
| 默认（auto） | 终端 → 人类表格；非终端 → AI 紧凑格式 | `emb keil files project.uvprojx -t "Target 1"` |
| `--ai` | 强制 AI 紧凑格式，最省 token | `emb --ai keil config project.uvprojx -t "Target 1"` |
| `--json` | 强制结构化 JSON，给 jq / 脚本消费 | `emb --json keil info project.uvprojx` |
| `--human` | 强制人类表格（输出被捕获时也想要表格） | `emb --human keil files project.uvprojx` |

`--ai` / `--json` / `--human` 互斥，不可同时使用。JSON 是 pretty 输出，体积约为 AI 的 1.5~4 倍（`keil files` 实测 35.8 KB vs 20.1 KB），只在需要程序化解析时用。

AI 模式输出示例：

```
device.name:STM32H750XBHx
output.name:signal_project
output.hex:yes
ccompiler.optim:4 (-O3) [0=<default> 1=-O0 2=-O1 3=-O2 4=-O3 5=-Ofast 6=-Os balanced 7=-Oz image size 8=-Omax]
ccompiler.wlevel:3 (AC5-like Warnings) [0=<unspecified> 1=No Warnings 2=All Warnings 3=AC5-like Warnings 4=MISRA Compatible]
```

表格用 `|` 分隔，单元格内的 `|` 会被转义成 `\|`；多段输出（`keil map`、`keil locate`、`keil info -t`、`debug keil`）之间用一行 `---` 分隔——不能用空行判断段界，因为 raw XML / UV4 日志本身可能含空行。

`--json` 每次调用只输出**一个** JSON 文档，且带 `kind` 判别字段，消费方按一个字段分发即可，不必按命令猜形状：

| kind | 其余字段 | 命令 |
|---|---|---|
| `table` | `headers`、`rows` | `keil info`（不指定 target）、`keil groups`、`keil files`、`keil map --lookup`、`ioc info`、`serial scan`、`serial daemon list`、`serial daemon history` |
| `kv` | `entries`（`[key, value]` 数组） | `keil config`、`keil build`、`ioc get <前缀>`、`config set` / `unset` |
| `list` | `items` | `keil defines`、`keil includes` |
| `message` | `value` | 写类命令的 `"ok"`、`ioc get <精确键>`、`keil locate` 的原文片段、`serial` 的空态提示 |
| `sections` | `sections`（上面几种的数组） | `keil map`、`keil locate`、`keil info -t <T>`（KV 头 + 可选的内存区表）、`debug keil`（KV 头 + dump + 出错时的 UV4 日志）—— 一条命令要输出多块内容时。`keil map --lookup` 是个例外：它只回答一件事，所以是单个 `table` |

```bash
$ emb --json ioc get project.ioc Mcu.Name     # 精确命中 → message
{ "kind": "message", "value": "STM32H750XBHx" }

$ emb --json ioc get project.ioc GPIO         # 前缀命中 → kv
{ "kind": "kv", "entries": [["GPIO.groupedBy", "Show All"]] }

$ emb --json keil map Objects/out.map | jq '.sections[] | .kind'
"kv"
"table"
```

## 完整命令参考

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
                                             # 分析 armlink .map（ROM/RW/ZI + 执行区域）
                                             # --lookup 把地址翻成符号：符号 + 偏移 + 类型 + 对象
emb keil locate <path> -t <target> <element> [-p <project>]
                                             # 定位 XML 节点（start_line / end_line /
                                             # Read Offset / Read Limit + 原始片段）
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
                               # 默认解码 \r \n \t \0 \e \xHH；--raw 按字面发
emb serial poke <port> <data> [--wait <ms>] [--hex|--raw] [--show-hex]
                               [-b <baud>] [-d <bits>] [-p <parity>] [-s <stop>]
                               # 一次调用内「清缓冲 → 写 → 等 → 读」
emb serial recv <port> --timeout <ms> [--show-hex] [-b <baud>] [-d <bits>] [-p <parity>] [-s <stop>]
emb serial daemon start <port> [-b <baud>] [--id <name>] [-d <bits>] [-p <parity>] [-s <stop>]
                               [--ttl <sec>] [--idle-timeout <sec>]
emb serial daemon list
emb serial daemon send <id> <data> [--hex|--raw]
emb serial daemon poke <id> <hex> [--wait <ms>] [--repeat <n>] [--interval <ms>] [--show-hex]
                               # data 恒为 hex；一次调用内「清缓冲 → 写 → 等 → 读」
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
emb debug openocd [args...]                       # 透传参数给 OpenOCD（需自备 openocd）
emb debug keil <path> [-t <target>] [options]     # UV4 -d 脚本化调试
  --regs                    读 CPU 寄存器 R0-R3/R12-R15/xPSR
  --read <addr[%size]>      读内存（默认 4 字节，%N 指定字节数）
  --dump <start,end>        dump 内存范围
  --break <addr|func>       设断点
  --run-to <addr|func>      运行到该处再取后面的采样（发 `G ,<sym>`，临时断点）
  --step <N> / --pstep <N>  单步 N 次（步入 / 步过）
  --ini <file>              自定义 .ini（须含自己的 EXIT）
  --timeout <sec>           UV4 -d 超时（默认 60）
  --no-reset                跳过初始 RESET
  --no-download             不下载：读板子上正在运行的固件（临时清下载开关，跑完还原）
```

> `debug keil` 走工程已配的调试器（`.uvoptx` 的 `<pMon>`，如 CMSIS-DAP），**批处理模式**（跑一段 .ini 脚本 dump 结果后退出），不是实时交互。要实时单步用 openocd + gdb。详见 `skills/keil-dev/references/debug.md`。

## 配置

`emb` 使用 TOML 配置文件 `.embconfig.toml`，按优先级加载：

1. `~/.embconfig.toml`（用户全局，优先级高）
2. `./.embconfig.toml`（当前项目，优先级低）

支持的配置项：

| Key | 用途 |
|-----|------|
| `keil_path` | UV4.exe 完整路径 |
| `cubemx_path` | STM32CubeMX.exe 完整路径 |

未配置时，工具会自动搜索常见安装路径和环境变量（`UV4_PATH`、`KEIL_PATH`、`CUBEMX_PATH`）。

## 构建和测试

```bash
# 开发构建
cargo build

# Release 构建
cargo build --release

# 运行测试
cargo test

# Lint
cargo clippy
```

## 项目结构

```
emb/
├── Cargo.toml
├── src/
│   ├── main.rs              # CLI 入口、clap 参数解析
│   ├── config.rs            # 配置管理 (.embconfig.toml)
│   ├── keil/
│   │   ├── mod.rs           # 命令路由
│   │   ├── parser.rs        # .uvprojx/.uvmpw XML 解析
│   │   ├── editor.rs        # XML 修改 (CRUD)
│   │   ├── builder.rs       # UV4.exe 调用（零 CPU 看门狗 / 日志流）
│   │   ├── map.rs           # .map 解析（总量 / 执行区域 / 符号表 + 地址查找）
│   │   ├── locate.rs        # XML 节点定位（行号 + 原始片段）
│   │   └── probe.rs         # 探针预检（.uvoptx 的 pMon vs PnP 枚举）
│   ├── ioc/
│   │   ├── mod.rs           # 命令路由
│   │   ├── parser.rs        # .ioc Properties 解析
│   │   ├── editor.rs        # 键值修改
│   │   └── generator.rs     # STM32CubeMX CLI 调用
│   ├── serial/
│   │   ├── mod.rs           # 命令路由
│   │   ├── port.rs          # 端口扫描/打开/收发
│   │   ├── daemon.rs        # 后台守护进程（TTL / idle 超时 / poke）
│   │   ├── protocol.rs      # ASCII/HEX 编解码
│   │   └── spawn.rs         # 句柄白名单的 detach 进程（Windows）
│   ├── debug/
│   │   ├── mod.rs           # 命令路由
│   │   ├── openocd.rs       # OpenOCD 接口 (stub)
│   │   └── keil_debug.rs    # UV4 -d 脚本化调试（.ini 生成 / uvoptx·uvprojx 补丁）
│   └── output/
│       ├── mod.rs           # OutputFormat 枚举
│       ├── human.rs         # 人类友好格式
│       ├── ai.rs            # AI 紧凑格式
│       └── json.rs          # JSON 格式
├── tests/                   # CLI 级集成测试（驱动真实二进制）
├── skills/
│   └── keil-dev/            # Claude Code Skill（SKILL.md + references/）
└── docs/
    └── plan/
        └── 测试验收_规格.md  # 测试验收规范
```

## 技术栈

- **Rust** 2021 edition
- **clap** 4 — CLI 参数解析
- **roxmltree** / **xmltree** — Keil XML 解析/写入
- **serialport** 4 — 跨平台串口通信
- **serde** / **serde_json** / **toml** — 序列化/配置
- **comfy-table** — 终端表格渲染
- **uuid** — Daemon ID 生成

## Claude Code Skills

仓库自带 `skills/keil-dev/` —— 教 AI 用 emb 高效操作 Keil 工程：

- **任务→路径决策**：查看 / 改值 / 增删节点 / 编译 / 调试 / map 各走哪条 emb 命令
- **locate + Read + Edit 工作流**：改 3000+ 行的 `.uvprojx` 省 ~300x token，不破坏 XML 格式
- **UV4 -d 脚本化调试**：读寄存器 / 内存 / `--run-to` 取样 / 单步 dump，含 `--no-download` 与批处理边界
- **排障指引**：编译假死（零 CPU 看门狗）、探针不匹配、daemon 超时与 `poke`、`map --lookup` 翻译 HardFault 地址
- editor 格式警告、`emb keil locate` 偏移、daemon 管道等踩坑条目

采用 progressive disclosure：SKILL.md 是精炼决策指引，命令细节在 `references/`（commands / editing / debug / serial / install）按需读。

**安装**：见 [`skills/keil-dev/references/install.md`](skills/keil-dev/references/install.md)，或用你的 skill 安装工具指向 `skills/keil-dev/`。

> 交互式单步调试用 openocd + gdb（若本机装了 `openocd-debug` skill 则直接用它），与 `emb debug keil` 的批处理模式互补。

## 许可证

MIT
