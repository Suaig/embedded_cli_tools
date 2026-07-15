---
name: keil-dev
description: Keil MDK 嵌入式工程（.uvprojx / .uvmpw）的完整开发——编译/烧录、查看与修改工程配置、增删源文件与宏、分析 .map 内存布局、在大型 XML 工程文件里精准定位做编辑、以及 UV4 -d 脚本化调试（读寄存器/内存/断点/单步 dump）。配套 CLI 是 emb（Rust 二进制）。当工作区含 .uvprojx 或 .uvmpw，或用户提到 Keil/MDK/UV4/ARMCLANG/armclang/armcc/scatter file/.axf/.map/Flash Download/CMSIS-DAP 时，必须使用本 skill——优先用 emb 命令，不要手动翻 XML、不要猜 UV4 命令行参数。
---

# Keil MDK 工程开发

配套 CLI：`emb`（Rust）。本 skill 教你怎么用 emb 高效操作 Keil 工程——而不是手动翻 3000 行 XML 或猜 UV4 命令。

## 前置

`emb` 需可用（`emb --version` 能跑通）。构建、加 PATH、配 Keil 路径见 **references/install.md**。Keil 装在 `C:\Keil_v5` 通常零配置。

## 核心：用户要做什么 → 走哪条路径

遇到 Keil 工程任务，先判断类型，对号入座（命令参数记不准就查 references/commands.md）：

| 用户要做 | 走这条 | 详见 |
|---|---|---|
| **看**工程配置（芯片/宏/include/文件分组） | `emb keil info / defines / includes / groups / files` | commands.md |
| **改一个值**（加宏、优化等级、scatter file） | ★ `emb keil locate` → Read → Edit | editing.md |
| **加/删源文件、分组** | `emb keil file-add / group-add / file-exclude …` | editing.md |
| **编译 / 烧录** | `emb keil build / rebuild / flash` | commands.md |
| **看内存用量 / 布局** | `emb keil map xxx.map` | commands.md |
| **读硬件状态**（寄存器/内存，调试） | `emb debug keil --regs / --read` | debug.md |
| **查/改 CubeMX .ioc** | 直接读 .ioc（小文件）+ `emb ioc get` | commands.md |

## 三条最关键的原则

**1. 改 .uvprojx 的值，用 locate+Edit，不要 Read 全文。**
`.uvprojx` 是 3000+ 行 XML，Read 全文 ~37000 token。`emb keil locate` 给你行号 + 原始片段，Read 那几行 + Edit 替换，上下文开销约为全读的 **1/300**，且不破坏 XML 格式。这条是本 skill 省 token 的核心，务必走。详见 editing.md。

**2. editor 命令（file-add / group-add 等）会破坏 XML 格式。**
editor 底层用 xmltree，会重排属性、丢 `xsi:` 前缀、删空行。Keil 能正常编译，但 `git diff` 噪音巨大。所以判断标准很简单：**改值 → locate+Edit；增删整节点 → editor**。详见 editing.md。

**3. debug keil 是批处理，不是实时交互。**
`emb debug keil` 跑一段 .ini 脚本（RESET → 读/dump → EXIT）拿结果，不是「敲一步、执行一步、停下等下一条」。要实时交互单步（像 IDE 那样），用 **openocd-debug** skill（openocd + gdb）。两者调试同一个 `.axf`，互补。详见 debug.md。

## 关键陷阱（高频踩坑）

- `emb keil locate` 返回 `start_line`，Read 时 **offset 直接填 start_line**（它是 1-based 起始行号，**不要减 1**）。
- `emb debug keil` 每次会**下载 flash + halt CPU**（工程 `UpdateFlashBeforeDebugging=1` 时），耗时。
- `--run-to` / 裸运行后断点没命中，程序一直跑，靠 `--timeout` 兜底（exit 124）。断点用具体函数名/地址。
- 多 target 工程 `-t <T>` 必填，否则操作第一个 target。
- emb 找 UV4 顺序：`emb config keil_path` > 环境变量 > `C:\Keil_v5` > 扫描 Program Files。非标准安装位置配一次 config 即全局生效（build/flash/debug 都读它）。

## references（按需读，不要一次全读）

| 何时读 | 文件 |
|---|---|
| emb 不可用 / 要配 Keil 路径 | references/install.md |
| 命令参数记不准 | references/commands.md |
| 要改 .uvprojx（locate 工作流 / editor 格式警告） | references/editing.md |
| 要调试（UV4 -d 边界 / 命令 / .ini） | references/debug.md |
