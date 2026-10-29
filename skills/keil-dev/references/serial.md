# emb serial 速查

串口的扫描、收发与常驻 daemon。三种用法：

- **`serial poke`**：一次性的「发 → 等 → 读」，**不需要 daemon**，短问答首选
- **`serial send` / `recv`**：一次性单向收发，各自开关端口
- **`daemon`**：常驻持有端口并留收发历史，适合连续交互 / 长期监控

## 端口是位置参数，不叫 `--port`

所有 `serial` 子命令都把端口当**位置参数**：

```bash
emb serial send COM7 48656c6c6f --hex     # 对
emb serial send --port COM7 ...           # 错：clap 报 unexpected argument
```

（顶层那个 `--port` 只服务于内部的 detached 子进程，不对外。）

## 数据怎么表示

`--hex` 只表示**输入是十六进制**；显示十六进制用 `--show-hex`。两个名字不再混用。

| flag | 含义 |
|---|---|
| 默认 | 字符模式，解码反斜杠转义：`\r` `\n` `\t` `\0` `\e` `\\` `\xHH` |
| `--hex` | 输入按十六进制解析（`76657273696f6e0d`） |
| `--raw` | 关掉转义解码，按字面发 |
| `--show-hex` | 只影响**回复的显示**为十六进制 |

`--hex` 与 `--raw` 互斥。未知转义原样保留（`C:\data` 不会被改），`\x` 必须跟两位十六进制。

letter shell 只认 `\r` 作回车（不是 `\n`），所以现在可以直接写 `"version\r"`，不必再手工转 hex。二进制数据才用 `--hex`。

## 一次性收发与问答

| 命令 | 说明 |
|---|---|
| `emb serial scan` | 列出端口与适配器名 |
| `emb serial send <PORT> <DATA> [--hex\|--raw] [-b BAUD] [-d N] [-p P] [-s N]` | 打开 → 写 → 关闭 |
| `emb serial recv <PORT> -t <MS> [--show-hex] [...]` | 打开 → 读 MS 毫秒 → 关闭；**0 字节返回非零** |
| `emb serial poke <PORT> <DATA> [--wait MS] [--hex\|--raw] [--show-hex] [...]` | 清缓冲 → 写 → 等 → 读，一次调用完成 |

`send` 写完就关端口，所以**别拿它跟 `recv` 配对等应答**——中间端口已经关了。要问答就用 `serial poke`，不必先起 daemon。

`-d` 数据位、`-p` 校验（`none` / `odd` / `even`）、`-s` 停止位——STM32 的 ROM Bootloader 要 **8E1**，就 `-p even`。

端口打不开时错误里会带端口名、提示，**以及当前实际存在的端口列表**（端口名漂移——上个星期是 COM20、今天是 COM5——一眼就能纠正）。有个坑：Windows 上「端口不存在」和「端口被别的程序占着」在 serialport 里是**同一个**错误种类（`ERROR_ACCESS_DENIED` 被折叠进 `NoDevice`），所以提示会把两种可能都列出来。看到 `Access is denied` 别默认是权限问题，先想想是不是有 daemon / Keil / 串口终端占着。

## 常驻 daemon

```bash
emb serial daemon start COM7 --id fan --ttl 600           # 10 分钟后自我了断
emb serial daemon start COM7 --id fan --idle-timeout 120  # 闲置 2 分钟自我了断
emb serial daemon poke  fan 7f --wait 50                  # data 恒为 hex；等 50ms 读回
emb serial daemon send  fan "version\r"                   # 字符模式同样解码转义
emb serial daemon send  fan 76657273696f6e0d --hex        # 或按 hex 发
emb serial daemon read  fan [--show-hex]                  # 取走未读数据
emb serial daemon history fan [-l N] [--clear]            # 收发历史
emb serial daemon list                                    # Status / Limits 两列
emb serial daemon stop  fan
```

- **`daemon poke` 的 `<data>` 恒为 hex**（这是唯一一个输入不接受转义的命令）；`--show-hex` 控制回复显示。没有应答时输出 `(no reply)`，这不是错误。`--repeat N --interval MS` 可重复。
- **`--ttl <s>` / `--idle-timeout <s>` 是给"agent 忘了关"准备的**。`ttl` 是硬上限；`idle` 只在**没有任何其它 emb 命令碰过它、且端口也没有数据进来**时才计时（`send` / `read` / `poke` / `history` 都算活动）。到点后 daemon 自己退出、释放端口，`list` 里 Status 会写 `stopped (ttl)` / `stopped (idle)`。
- `--id` 是自定义短名，省略则生成 uuid；多实例可同时跑。id 必须是**普通名字**——含路径分隔符、盘符或 `.` / `..` 会被直接拒绝，因为它是状态目录名。
- `start` 会**确认子进程真的起来了**才开始报成功：端口被占或名字写错时，它会给出原因并返回非 0。
- `send` 的 `ok` 表示**字节已经交给 daemon 写出去**了。daemon 写失败（端口掉线、驱动卡死）时 `send` 会报错，不再静默丢数据。
- `read` 是**取走**语义（读过就没了），`history` 是留档。未读数据攒到 4 MB 后会丢掉最旧的。
- **用完 `stop`**；忘了也不要紧，`--ttl` / `--idle-timeout` 会兜底。端口被 daemon 占着时别的工具（Keil 下载、OTA 上位机）打不开。
- 默认 115200 8N1。

### 一次问答怎么发

| 场景 | 用法 |
|---|---|
| 短命令问答（≤2 s） | `emb serial poke COM7 "version\r" --wait 800`——无 daemon，一次调用完成 |
| 需要历史 / 连续交互 | `daemon poke <id> <hex> --wait 800` |
| 长命令（`cp` / `mkfs` / 格式化） | `daemon send` + **恰好一个** `read`，等待时间给足 |
| 要原始字节 | `daemon read` / `recv --show-hex` |

- **绝不同时起两个 `read`**：`read` 是取走语义，先到的拿走全部，另一个看到空——极易误判成"命令卡住"。
- **`poke` 不会自动补行结束符**：对 letter shell 发命令必须自己带 `\r`（如 `"version\r"`）。漏了会残留在设备行缓冲里，下一条拼成 `versionversion`；补齐前先发 `03`（Ctrl-C）清行。
- 发长命令 / 长文件时，hex 这样生成（字符模式写不下时用）：

  ```bash
  hex=$(printf '%s\r' "$cmd" | od -An -v -tx1 | tr -d ' \n')
  emb serial daemon send <id> "$hex" --hex
  ```

  **`-v` 不能省**：GNU `od` 默认把连续重复行折叠成 `*`，长重复内容会生成奇数长度的 hex，emb 会如实拒绝（报 `hex string must have even length`）。

### 输出可以进管道

`daemon start` 及其它命令都可以安全地接管道、放进 `$( )`：

```bash
ID=$(emb serial daemon start COM7 --id fan)   # 直接捕获，$( ) 会剥掉尾部换行
```

（0.1.0 之前的构建不行：daemon 会继承并一直持有调用方的 stdout 管道，`| head` 永不返回 —— 根因见 `src/serial/spawn.rs` 的模块注释。老版本必须 `>/dev/null 2>&1 </dev/null`。）

## 驱动 letter shell

字符模式直接写 `\r`：

```bash
emb serial poke COM7 "version\r" --wait 800
```

对应的 hex 仅作二进制场景参考：

| 想发的 | hex |
|---|---|
| 回车 | `0d` |
| `version\r` | `76657273696f6e0d` |
| `ls /dev\r` | `6c73202f6465760d` |
| Ctrl-C | `03` |

`read` 回来的数据里常混着 NUL 与 ANSI 转义序列（shell 开机清屏 `ESC[2J ESC[1H`，前面还可能有一个 `0x00`），取文本要 `cat -v` 或 `tr -d '\0'`。
