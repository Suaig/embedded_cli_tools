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

## 常驻 daemon

```bash
emb serial daemon start COM7 --id fan >/dev/null 2>&1 </dev/null
emb serial daemon send  fan 76657273696f6e0d --hex    # 发 "version\r"
emb serial daemon read  fan                           # 取走未读数据
emb serial daemon history fan [-l N] [--clear]        # 收发历史
emb serial daemon list
emb serial daemon stop  fan
```

- `--id` 是自定义短名，省略则生成 uuid；多实例可同时跑。
- `read` 是**取走**语义（读过就没了），`history` 是留档。
- **用完必须 `stop`**：端口被 daemon 占着时别的工具（Keil 下载、otf 上位机）打不开。
- 控制字符 / 二进制一律用 `--hex`——转义没法直接写在参数里。
- 默认 115200 8N1。

### daemon start 的输出必须重定向掉

```bash
emb serial daemon start COM7 --id fan >/dev/null 2>&1 </dev/null
                                       ^^^^^^^^^^^^^^^^^^^^^^^
```

不带这段，调用方会被**永久挂死**：daemon 是 detach 出来的常驻进程，而 Windows 上 Rust 的 `std::process::Command` 会把所有可继承句柄都传给子进程、**没有白名单**（[rust-lang/rust#161158](https://github.com/rust-lang/rust/issues/161158)）。于是它继承并一直持有调用方的 stdout 管道，`| head` / `| cat` 永远等不到 EOF——外层 `timeout` 也救不了，它只杀父进程，而父进程早就退了。

`emb` 现已改用 `CreateProcessW` + `PROC_THREAD_ATTRIBUTE_HANDLE_LIST`，只放行三个 NUL 句柄（`src/serial/spawn.rs`）：

- **已修复的构建**：管道里调用正常（实测 350 ms 返回）。
- **更早的构建**：必须重定向。

## 驱动 letter shell

letter shell 只认 `\r` 作回车（不是 `\n`），所以命令用 `--hex` 发：

| 想发的 | hex |
|---|---|
| 回车 | `0d` |
| `version\r` | `76657273696f6e0d` |
| `ls /dev\r` | `6c73202f6465760d` |
| Ctrl-C | `03` |

`read` 回来的数据里常混着 NUL 与 ANSI 转义序列（shell 开机清屏 `ESC[2J ESC[1H`，前面还可能有一个 `0x00`），取文本要 `cat -v` 或 `tr -d '\0'`。
