---
name: keil-dev
description: Keil MDK 嵌入式工程（.uvprojx / .uvmpw）的完整开发——编译/烧录、查看与修改工程配置、增删源文件与宏、分析 .map 内存布局、在大型 XML 工程文件里精准定位做编辑、UV4 -d 脚本化调试（读寄存器/内存/断点/单步 dump）、以及串口扫描/收发/常驻 daemon。配套 CLI 是 emb（Rust 二进制）。当工作区含 .uvprojx 或 .uvmpw，或用户提到 Keil/MDK/UV4/ARMCLANG/armclang/armcc/scatter file/.axf/.map/Flash Download/CMSIS-DAP，或要操作串口（COM 口、serial、串口收发、daemon）时，必须使用本 skill——优先用 emb 命令，不要手动翻 XML、不要猜 UV4 命令行参数、不要自己写串口脚本。
---

# Keil MDK 工程开发

配套 CLI：`emb`（Rust）。本 skill 教你怎么用 emb 高效操作 Keil 工程——而不是手动翻 3000 行 XML 或猜 UV4 命令。

## 前置

`emb` 需可用。先 `command -v emb`：失败时问用户 emb 仓库位置，用 `<仓库>/target/release/emb.exe`，**不要重装**——install.md 只用于确实还没构建过的情况。Keil 装在 `C:\Keil_v5` 通常零配置。

**每次会话先确认现场，不要沿用记忆里的值**：`emb keil info <proj>` 取 target 名（多 target 工程必须用真实名字；文档里过时的 `-t TX` 会被拒绝并列出可用 target），`emb serial scan` 取当前 COM 口（端口会漂移，上周 COM20 这周可能是 COM21）。

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
| **收发串口 / 一次问答 / 常驻监听** | `emb serial scan / poke / send / recv / daemon …` | serial.md |

## 三条最关键的原则

**1. 改 .uvprojx 的值，用 locate+Edit，不要 Read 全文。**
`.uvprojx` 是 3000+ 行 XML，Read 全文 ~37000 token。`emb keil locate` 给你行号 + 原始片段，Read 那几行 + Edit 替换，上下文开销约为全读的 **1/300**，且不破坏 XML 格式。这条是本 skill 省 token 的核心，务必走。详见 editing.md。

**2. 除 `file-add` 外的 editor 命令会破坏 XML 格式。**
`group-add` / `file-remove` / `defines-add` 等底层用 xmltree，会重排属性、丢 `xsi:` 前缀、删空行。Keil 能正常编译，但 `git diff` 噪音巨大；`file-add` 已改成按缩进做文本插入，diff 就只有新增的那几行。判断标准：**改值 → locate+Edit；增删整节点 → editor**。详见 editing.md。

**3. debug keil 是批处理，不是实时交互。**
`emb debug keil` 跑一段 .ini 脚本（RESET → 读/dump → EXIT）拿结果，不是「敲一步、执行一步、停下等下一条」。要实时交互单步（像 IDE 那样），用 openocd + gdb（若本机装了 `openocd-debug` skill 则直接用它）。两者调试同一个 `.axf`，互补。详见 debug.md。

## 命令调用纪律

Agent 最常见的浪费不是命令用错，而是把命令拼成一条、再把输出吞掉。按下面做：

**1. 一条命令只做一件事。**
不要 `emb keil build ... && sleep 10 && emb serial daemon poke ... | grep xxx`。build、poke、read 各跑一次。每一步的**退出码和完整输出**都是诊断证据；串成一条后只剩最后一个命令的退出码，前一步失败会被静默跳过。

**2. 不要用 `| tail` / `| head` / `| grep` 截断要拿来下结论的输出。**
`emb` 的输出本来就短（默认 AI 格式下 `keil files` 也只是一张紧凑表），完整读的代价小于误判。必须筛时写成 `2>&1 | tee /tmp/x.log | grep ...`，然后读完整 log——**注意管道会吞掉 emb 的退出码**，`$?` 是管道最后一个命令的。

**3. build / flash / debug 是同步的，不要 sleep 等。**
命令返回即结束。串口问答用 `poke` 自带的 `--wait`，不要 `sleep` + `recv`。需要等设备完成擦写/校验时，用 `poke` 轮询提示符，不要固定的 `sleep N`。

**4. 先看完整输出，再 grep。**
grep 是二次筛选，不是第一次读取。`poke` 没应答时输出 `(no reply)`，被 `grep` 过滤后就是空——那不是"没执行"，是"没匹配"。

**5. `daemon start` 直接捕获 id。**
`ID=$(emb serial daemon start COMx --id x)`，不要接 `| head`。

## 关键陷阱（高频踩坑）

- `emb keil locate` 输出里的 **`Read Offset` / `Read Limit`** 直接就是 Read 的 `offset` / `limit`，照抄即可，不要自己换算（`offset` 是 1-based）。
- `emb debug keil` 每次会**下载 flash + halt CPU**（工程 `UpdateFlashBeforeDebugging=1` 时），耗时。只想读寄存器、不想冲掉正在跑的固件就加 **`--no-download`**（临时清下载开关，跑完还原）。
- 没有探针时 UV4 只给一句 `Target DLL has been cancelled`。emb 会在失败后补充"工程配的是哪种探针、本机检测到几个"；`flash` 则在调用前比对并警告。
- `--run-to` / 裸运行后断点没命中，程序一直跑，靠 `--timeout` 兜底（exit 124）。断点用具体函数名/地址。
- 多 target 工程 `-t <T>` 必填，否则静默操作第一个 target；要一次编完所有 target 用 `--all-targets`（仅 `build` / `rebuild`）。
- emb 找 UV4 顺序：`emb config keil_path` > 环境变量 > `C:\Keil_v5` > 扫描 Program Files。非标准安装位置配一次 config 即全局生效（build/flash/debug 都读它）。
- `emb serial daemon start` 的输出可以安全捕获：`ID=$(emb serial daemon start COMx --id x)`。端口是**位置参数**，不叫 `--port`。详见 serial.md。

## references（按需读，不要一次全读）

| 何时读 | 文件 |
|---|---|
| emb 不可用 / 要配 Keil 路径 | references/install.md |
| 命令参数记不准 | references/commands.md |
| 要改 .uvprojx（locate 工作流 / editor 格式警告） | references/editing.md |
| 要调试（UV4 -d 边界 / 命令 / .ini） | references/debug.md |
| 要用串口（扫描 / 收发 / daemon / 驱动 letter shell） | references/serial.md |
