# emb keil / ioc 命令速查

`<proj>` = `.uvprojx` 路径；`<T>` = target 名。`info` / `build` / `rebuild` / `clean` / `flash` / `debug` 的 `-t` 可省（省时静默用第一个 target）；`config` / `defines` / `includes` / `groups` / `files` 及写类命令的 `-t` 是 clap 必填。输出格式**默认自动**：stdout 接终端用表格，被管道 / Agent 捕获用 AI 紧凑格式；`--ai` / `--json` / `--human` 可强制。Agent 通常不用加任何 flag。

`.uvmpw` 工作区：所有**接受工程路径**的 keil 子命令都接受 `-p <project>`（子项目文件名如 `Boot.uvprojx`，或 `.\Boot\Boot.uvprojx` 完整相对路径）；不带 `-p` 时取工作区里的**第一个**子项目。（`emb keil map` 例外——它接收的是 `.map` 文件，与工作区无关。）

## 查看（只读）

| 命令 | 用途 |
|---|---|
| `emb keil info     <proj> [-t <T>]` | 工程概览（targets / 芯片 / 输出目录）；带 `-t` 时另附该 target 的内存区表 |
| `emb keil config   <proj> -t <T> [category]` | target 配置 |
| `emb keil defines  <proj> -t <T>` | 预处理宏 |
| `emb keil includes <proj> -t <T>` | include 路径 |
| `emb keil groups   <proj> -t <T>` | 源文件分组 |
| `emb keil files    <proj> -t <T> [-g GROUP]` | 文件列表（可按 group 过滤） |

上表均可加 `[-p <project>]`。

`files` 的 `Status` 列：`excluded` 表示该文件在 Keil 里取消勾选了「参与编译」（`IncludeInBuild=0`），不是文件缺失。判断一个文件是否真在编，看三处：构建目录有没有 `.o`、build log 是否提到、`.map` 是否命中。

`config` 的 `category` 只接受顶层分类（`device`/`output`/`ccompiler`/`asm`/`linker`/`memory`），**不接受具体键**（`ccompiler.optim` 会报错并列出可选分类）。`config <category>` 与 `files -g <group>` 在无匹配时返回非零退出码，不会静默给空结果。

`config memory.irom` / `iram` / `xram` 读的是 Keil 的老式标签，其中 `xram` 在多数工程里是空的历史字段——**别拿它判断"没有第二块 RAM"**。真正生效的内存区在 `<OCR_RVCT1..10>` 里，用 `keil info -t <T>` 看（列出所有 size 非零的区，含 ROM/RAM 类型）。

## 定位（省 token，见 editing.md）

```
emb keil locate <proj> -t <T> defines       # 宏节点（start/end line + Read Offset/Limit + raw）
emb keil locate <proj> -t <T> ScatterFile   # 任意配置标签
emb keil locate <proj> -t <T> File          # 多行元素（start_line + end_line）
```

`locate` 的输出里 `Read Offset` / `Read Limit` 就是 Read 工具要填的两个参数，直接照抄，不要自己换算。

可加 `[-p <project>]`。`locate` 是**只读**的，返回行号供 Read + Edit 使用，不会重写 XML。

## 修改工程（editor 快速模式，见 editing.md 的格式警告）

```
emb keil defines-add    <proj> -t <T> MACRO
emb keil defines-remove <proj> -t <T> MACRO
emb keil includes-add    <proj> -t <T> path
emb keil includes-remove <proj> -t <T> path
emb keil group-add       <proj> -t <T> NAME
emb keil group-remove    <proj> -t <T> NAME
emb keil group-rename    <proj> -t <T> OLD NEW
emb keil file-add        <proj> -t <T> -g GROUP filepath
emb keil file-remove     <proj> -t <T> -g GROUP filename
emb keil file-exclude    <proj> -t <T> -g GROUP filename   # 不编译
emb keil file-include    <proj> -t <T> -g GROUP filename   # 恢复编译
emb keil config-set      <proj> -t <T> key value
```

## 构建 / 烧录

```
emb keil build   <proj> [-t <T>] [-p <project>] [--timeout S] [--stream] [--all-targets]   # 增量
emb keil rebuild <proj> [-t <T>] [-p <project>] [--timeout S] [--stream] [--all-targets]   # 全量重编译
emb keil clean   <proj> [-t <T>] [-p <project>] [--timeout S]
emb keil flash   <proj> [-t <T>] [-p <project>] [--timeout S] [--stream]   # Flash Download
```

**多 target 工程（如双槽 SlotA/SlotB）一次编完用 `--all-targets`**：遍历工程里所有 target，最后给一张汇总表（Target / Success / Errors / Warnings / Build Time / Output），任一失败返回非零。它与 `-t` 互斥，只支持 `build` / `rebuild`。只编一个 target 会让另一个的 hex 停留在上一轮。
返回 success / errors / warnings / program_size(Code/RO/RW/ZI) / output_file。UV4 码：0=成功，1=警告，2+=错误。`flash` 内部就是 `UV4 -f`，与 Keil GUI 的 Flash Download 等价。

**失败会返回非零退出码**，stderr 带错误/警告计数与 UV4 日志末尾几行。target 名写错会在调用 UV4 **之前**就被拒掉并列出可用 target，不会白跑一轮编译。

**卡死会自动中止（默认就开）**：UV4 连续 **30 秒零 CPU** 就被判定为卡住并杀掉——`-j0` 只隐藏窗口，挡不住 μVision 弹模态对话框，而对话框会让进程零 CPU 傻等，这正是"编译几分钟没输出"的真相。许可（licence）排队也是同一副景象。`--timeout S` 是额外的墙钟上限，默认 0（不限）——大工程全量重编译本来就要好几分钟，按时间杀是错的。

**`--stream` 把 UV4 日志实时回显到 stderr**（stdout 仍是那张结果表，`--json` 不受影响）。构建日志是 UV4 自己增量写的 `-o` 文件，emb 边等边跟读即可；一次大重编译的日志有几十 KB，所以默认关，只在排查慢/卡的时候开。

**`flash` 会先做探针预检**：读 `.uvoptx` 里该 target 的 `<pMon>`，和 Windows 枚举到的调试器对一下，不一致就在 stderr 打一行警告——不然 UV4 只会给 `Target DLL has been cancelled` / `Internal DLL Error`，既不说是哪个探针也不说实际接的是什么。判定不出来时**不报警**：宁可漏报，也不阻断本来能成功的烧录。

`.uvmpw` 下不带 `-p` 只会构建工作区第一个子项目；要构建别的子项目必须显式 `-p`。

## map 分析

```
emb keil map Objects/STM32H723/STM32H723.map                          # 总量 + 执行区域
emb keil map .../TX_SlotA.map --lookup 0x90034FCE                     # 地址 → 符号
emb keil map .../TX_SlotA.map --lookup 0x90034FCE,0x9003FFFF          # 逗号分隔多个
```
输出 ROM/RW/ZI 总量 + 各执行区域。核对内存布局（DTCM/AXI SRAM/D3 SRAM 分配）。

`--lookup` 是 **HardFault 之后第一件要做的事**：把 PC / LR 翻成函数名 + 偏移。两个坑它已经处理：符号在 map 的 **Local Symbols** 段（静态函数不在 Global 段），Thumb 符号值的 bit0 是 1 而 PC 不是（比较前要掩掉）。同一地址上既有函数又有 `[Anonymous Symbol] Section` 标记时取函数。

## 调试（UV4 -d 脚本化，见 debug.md）

```
emb debug keil <proj> -t <T> --regs
emb debug keil <proj> -t <T> --read 0x58001408[%16]
emb debug keil <proj> -t <T> --dump 0x20000000,0x20000100
emb debug keil <proj> -t <T> --break main [--run-to main] [--step 5] [--regs]
emb debug keil <proj> -t <T> --ini custom.ini [--timeout 60] [--no-reset]
```

## ioc（STM32CubeMX，辅助）

```
emb ioc info <file>.ioc          # 分类概览
emb ioc get  <file>.ioc SPI6     # 查某外设全部参数
emb ioc set  <file>.ioc KEY VAL
emb ioc rm   <file>.ioc KEY
emb ioc generate <file>.ioc      # 调 CubeMX 重新生成代码
```
`.ioc` 是 ~19KB 平铺 key=value，直接读为主，emb 辅助查询。改完必须 CubeMX regenerate。
