# emb debug keil：UV4 -d 脚本化调试

## 模式本质：批处理，不是实时交互

emb debug keil 走 `UV4 -d + .ini 脚本`：每次调用生成一段 `.ini`（`RESET → LOG → 读/dump → LOG OFF → EXIT`），UV4 一次性跑完退出。

这是**批处理**——跑完拿到 dump 结果。**不是**「敲一条 step、执行一步、停下等下一条」的交互式调试。UV4 没有 stdin 交互接口，这是 Keil 的固有限制。

要**实时交互单步**（像 IDE 那样），用 openocd + gdb（GDB MI 是成熟交互协议）；若本机装了 `openocd-debug` skill 就直接用它。

## 何时用 debug keil vs openocd

| 场景 | 用 |
|---|---|
| 读某时刻寄存器/内存、dump 状态、自动化验证 | **debug keil**（不用装额外工具） |
| 实时单步、实时看变量、交互查问题 | openocd + gdb（或已装的 openocd-debug skill） |

两者调试同一个 `.axf`，互补。

## 副作用（每次调用）

- **会下载 flash + halt CPU**：工程 `UpdateFlashBeforeDebugging=1` 时，每次 debug 都重新烧录。耗时（下载+验证）。只想连上读寄存器、不想冲掉正在跑的固件时加 **`--no-download`**（实测 5 s vs 31 s，UV4 日志里没有 `Erase Done. Programming Done.`）。
- **改 .uvoptx / .uvprojx（都会还原）**：emb 临时改 `.uvoptx` 的 `<tIfile>` 挂载 .ini；`--no-download` 时再改一处——`.uvprojx` 的 `<UpdateFlashBeforeDebugging>`（Utilities 页「Update target before Debugging」，注意它在 **.uvprojx 不在 .uvoptx**）。跑完自动还原；实测 `.uvprojx` 逐字节回到原样。Keil 自身会改 .uvoptx 别的字段（探针探测 / 断点地址 / 窗口配置，例如 `<aLwin>`），属正常、无害。
- ⚠️ **`--no-download` 特意不动 `.uvoptx` 的 `<tLdApp>`**（Debug 页「Load Application at Startup」）。实测它**不影响**下载（`tLdApp=0` 时照样擦除+编程），但关掉它**会丢符号表**——`BS main` 直接报 `*** error 34: undefined identifier`，`--break`/`--run-to` 全废。真正管下载的是 `.uvprojx` 那一个。
- **没有探针时 UV4 只给一句 `Target DLL has been cancelled`**。emb 会在失败后补一行说明工程配的是哪种探针、以及本机一个都没检测到；`flash` 则在调用前就做同样的比对并警告。

## 读寄存器 / 内存

```
emb debug keil <proj> -t <T> --regs                       # R0-R3 / R12-R15 / xPSR
emb debug keil <proj> -t <T> --read 0x58001408             # 读 4 字节
emb debug keil <proj> -t <T> --read 0x20000000%16          # 读 16 字节（按 4 字节 word）
emb debug keil <proj> -t <T> --dump 0x20000000,0x20000100  # 内存范围
```

RESET 后读的是复位状态（外设多未初始化，读到的多是 0）。要读运行后状态，配 `--run-to <func>` 跑到某处再读。

## 断点 / 运行 / 单步（脚本化）

```
emb debug keil <proj> -t <T> --break main                 # 设断点（不运行）
emb debug keil <proj> -t <T> --run-to main                # 跑到 main 再继续执行后面的读/转储
emb debug keil <proj> -t <T> --step 5                     # 单步(步入) 5 次
emb debug keil <proj> -t <T> --pstep 3                    # 单步(步过) 3 次
# 组合：跑到 main 看寄存器
emb debug keil <proj> -t <T> --run-to main --regs
```

⚠️ **`--run-to` 发的是 `G ,<sym>`（临时断点），不是 `BS <sym>` + `G`。** 实测后者在真实目标上永远不命中——程序一路跑到 `--timeout`，最终以超时失败退出；`G ,main` 立刻停在 main。原因没深究，但形式就用 `G ,`。`--break` 的 `BS` 本身是好的（和 `--run-to` 一起用时确实先在 `--break` 那个符号停住），只是**单独 `G` 不带停止地址时不看断点列表**。

**想让程序"先跑一会儿"再取样**：Keil 的调试命令里**没有 HALT / DELAY**（停止执行只有断点或交互式 Ctrl-C），所以"跑 N 毫秒"表达不出来。可行做法是 `--run-to <一个稍后才执行到的符号>`（任务函数、周期性的中断处理函数等），命中后再读。`--no-reset` 只跳过 ini 里的 `RESET`，**不能**阻止 μVision 连接时自己复位目标。

## 自定义 .ini（高级）

```
emb debug keil <proj> -t <T> --ini my_debug.ini
```
自定义 .ini **必须含自己的 EXIT**，否则 UV4 卡住靠 timeout。可用的 Keil 调试命令：`RESET` / `G [addr]` / `BS addr` / `BC n` / `Tstep` / `Pstep` / `DISPLAY start,end` / `printf(..., _RDWORD(addr))` / `LOG > file` / `LOG OFF` / `EXIT`。

## 输出与排障

输出最多三段，**`--json` 下是一个合法 JSON 文档**（`kind: "sections"`），不是多段拼接：

1. `Target` / `Exit Code` / `Timed Out` —— 每次都输出
2. 过滤掉命令回显后的 dump 内容 —— 有内容时输出
3. **UV4 自身日志的末尾 20 行** —— 仅在 `Timed Out: yes` 或 `Exit Code != 0` 时输出

第 3 段是排障入口：`.ini` 的 dump 只记录调试命令的输出，而「连不上调试器」「target 不存在」「license 报错」这类问题只出现在 UV4 自己的日志里。该日志写在临时目录、命令结束即删，所以出问题时直接把尾部带出来。仍不够用时，用 `--ini` 换自定义脚本并自行 `LOG >` 到固定路径。

## 选项

- `--timeout 60`    UV4 -d 超时秒数（默认 60）。超时或 UV4 非零退出时，打印完 dump 后命令返回非零。
- `--no-reset`      跳过初始 RESET。
- `--no-download`   不进下载：临时清掉该 target 的下载开关，读的就是板子上正在跑的那份固件。跑完还原。输出里 `Download` 一行会写 `skipped (--no-download)`。

## 调试器配置

emb debug keil 走**工程已配的调试器**（Keil GUI 里 Options → Debug → Use 选的），emb 不需要额外指定。确认工程配的调试器：

```
grep pMon <proj>.uvoptx
# CMSIS_AGDI.dll   = CMSIS-DAP
# ULINK2CM3.dll    = ULINK
# JL2CM3.dll       = J-Link
```

注意：`.uvoptx` 里 `<Utilities><Flash1><DriverSelection>` 是 **flash download 驱动**，和调试器（Debug）是两回事。
