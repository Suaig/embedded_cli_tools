# emb serial 速查

串口的扫描、收发与常驻 daemon。**daemon 是主角**：它常驻持有端口并把收发写进历史，适合脚本 / 自动化驱动设备；`send` / `recv` 是一次性的，各自开关端口。

## 端口是位置参数，不叫 `--port`

所有 `serial` 子命令都把端口当**位置参数**：

```bash
emb serial send COM7 48656c6c6f --hex     # 对
emb serial send --port COM7 ...           # 错：clap 报 unexpected argument
```

（顶层那个 `--port` 只服务于内部的 detached 子进程，不对外，所以报错里能看到它、却用不了。）

## 一次性收发

| 命令 | 说明 |
|---|---|
| `emb serial scan` | 列出端口与适配器名 |
| `emb serial send <PORT> <DATA> [--hex] [-b BAUD] [-d N] [-p P] [-s N]` | 打开 → 写 → 关闭 |
| `emb serial recv <PORT> -t <MS> [--hex] [-b BAUD]` | 打开 → 读 MS 毫秒 → 关闭 |

`send` 写完就关端口，所以**别拿它跟 `recv` 配对等应答**——中间端口已经关了。要问答就用 daemon。

端口打不开时错误里会带端口名和提示。有个坑：Windows 上「端口不存在」和「端口被别的程序占着」在 serialport 里是**同一个**错误种类（`ERROR_ACCESS_DENIED` 被折叠进 `NoDevice`），所以提示会把两种可能都列出来。看到 `Access is denied` 别默认是权限问题，先想想是不是有 daemon / Keil / 串口终端占着。

## 常驻 daemon

```bash
emb serial daemon start COM7 --id fan >/dev/null 2>&1 </dev/null
emb serial daemon send  fan 76657273696f6e0d --hex    # 发 "version\r"
emb serial daemon read  fan                           # 取走未读数据
emb serial daemon history fan [-l N] [--clear]        # 收发历史
emb serial daemon list
emb serial daemon stop  fan
```

- `--id` 是自定义短名，省略则生成 uuid；多实例可同时跑。id 必须是**普通名字**——含路径分隔符、盘符或 `.` / `..` 会被直接拒绝，因为它是状态目录名。
- `start` 会**确认子进程真的起来了**才开始报成功：端口被占或名字写错时，它会给出原因并返回非 0，而不是打印一个 id 就走。
- `send` 的 `ok` 表示**字节已经交给 daemon 写出去**了。daemon 写失败（端口掉线、驱动卡死）时 `send` 会报错，不再静默丢数据。
- `read` 是**取走**语义（读过就没了），`history` 是留档。未读数据攒到 4 MB 后会丢掉最旧的——板子一直刷屏而没人 `read` 时不会把磁盘写满，但也别指望它当无限缓冲用。
- **用完必须 `stop`**：端口被 daemon 占着时别的工具（Keil 下载、otf 上位机）打不开。
- 控制字符 / 二进制一律用 `--hex`——转义没法直接写在参数里。
- 默认 115200 8N1。

### 输出可以进管道

`daemon start` 及其它命令都可以安全地接管道、放进 `$( )`：

```bash
ID=$(emb serial daemon start COM7 --id fan)   # 直接捕获，$( ) 会剥掉尾部换行
```

（0.1.0 之前的构建不行：daemon 会继承并一直持有调用方的 stdout 管道，`| head` 永不返回 —— 根因见 `src/serial/spawn.rs` 的模块注释。老版本必须 `>/dev/null 2>&1 </dev/null`。）

## 驱动 letter shell

letter shell 只认 `\r` 作回车（不是 `\n`），所以命令用 `--hex` 发：

| 想发的 | hex |
|---|---|
| 回车 | `0d` |
| `version\r` | `76657273696f6e0d` |
| `ls /dev\r` | `6c73202f6465760d` |
| Ctrl-C | `03` |

`read` 回来的数据里常混着 NUL 与 ANSI 转义序列（shell 开机清屏 `ESC[2J ESC[1H`，前面还可能有一个 `0x00`），取文本要 `cat -v` 或 `tr -d '\0'`。
