# emb debug keil：UV4 -d 脚本化调试

## 模式本质：批处理，不是实时交互

emb debug keil 走 `UV4 -d + .ini 脚本`：每次调用生成一段 `.ini`（`RESET → LOG → 读/dump → LOG OFF → EXIT`），UV4 一次性跑完退出。

这是**批处理**——跑完拿到 dump 结果。**不是**「敲一条 step、执行一步、停下等下一条」的交互式调试。UV4 没有 stdin 交互接口，这是 Keil 的固有限制。

要**实时交互单步**（像 IDE 那样），用 openocd-debug skill（openocd + gdb，GDB MI 是成熟交互协议）。

## 何时用 debug keil vs openocd

| 场景 | 用 |
|---|---|
| 读某时刻寄存器/内存、dump 状态、自动化验证 | **debug keil**（不用装额外工具） |
| 实时单步、实时看变量、交互查问题 | openocd-debug |

两者调试同一个 `.axf`，互补。

## 副作用（每次调用）

- **会下载 flash + halt CPU**：工程 `UpdateFlashBeforeDebugging=1` 时，每次 debug 都重新烧录。耗时（下载+验证）。
- **改 .uvoptx**：emb 临时改 `<tIfile>` 挂载 .ini，**跑完自动还原**。Keil 自身会改 .uvoptx 别的字段（探针探测 / 断点地址 / 窗口配置），属正常、无害。

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
emb debug keil <proj> -t <T> --break main                 # 设断点
emb debug keil <proj> -t <T> --run-to main                # 跑到 main（临时断点+G+清除）
emb debug keil <proj> -t <T> --step 5                     # 单步(步入) 5 次
emb debug keil <proj> -t <T> --pstep 3                    # 单步(步过) 3 次
# 组合：跑到 main 看寄存器
emb debug keil <proj> -t <T> --run-to main --regs
```

⚠️ `--run-to` / 裸运行后若断点没命中，程序一直跑，靠 `--timeout` 兜底（exit 124）。断点尽量用具体函数名/地址。

## 自定义 .ini（高级）

```
emb debug keil <proj> -t <T> --ini my_debug.ini
```
自定义 .ini **必须含自己的 EXIT**，否则 UV4 卡住靠 timeout。可用的 Keil 调试命令：`RESET` / `G [addr]` / `BS addr` / `BC n` / `Tstep` / `Pstep` / `DISPLAY start,end` / `printf(..., _RDWORD(addr))` / `LOG > file` / `LOG OFF` / `EXIT`。

## 选项

- `--timeout 60`  UV4 -d 超时秒数（默认 60）。超时返回 exit 124。
- `--no-reset`    跳过初始 RESET。

## 调试器配置

emb debug keil 走**工程已配的调试器**（Keil GUI 里 Options → Debug → Use 选的），emb 不需要额外指定。确认工程配的调试器：

```
grep pMon <proj>.uvoptx
# CMSIS_AGDI.dll   = CMSIS-DAP
# ULINK2CM3.dll    = ULINK
# JL2CM3.dll       = J-Link
```

注意：`.uvoptx` 里 `<Utilities><Flash1><DriverSelection>` 是 **flash download 驱动**，和调试器（Debug）是两回事。
