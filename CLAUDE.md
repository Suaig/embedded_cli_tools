# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 构建和测试

```bash
cargo build              # 开发构建
cargo build --release    # Release 构建
cargo test               # 运行全部测试
cargo test -p embedded_cli_tools -- <test_name>  # 运行单个测试
cargo clippy             # Lint
```

## 架构

`emb` 是一个面向嵌入式开发的 CLI 工具，采用单二进制 + 子命令架构。二进制名 `emb`，入口 `src/main.rs`。

### 模块层级

```
main.rs (clap CLI) → 模块路由 → handler → 领域逻辑 → output::display()
```

每个模块（`keil`/`ioc`/`serial`/`debug`/`config`）有自己的 `mod.rs` 作为路由层，将 CLI 枚举变体映射到领域函数调用。

### 输出格式系统

所有命令结果通过 `output` 模块统一渲染。`OutputValue` 有 4 种变体：`Message`、`KeyValue`、`Table`、`List`。三种格式（Human/AI/JSON）分别实现 `render()`。`OutputFormat` 在 `main.rs` 中由 `--ai`/`--json` flag 决定，默认 Human。

AI 模式下 `keil config` 的输出有特殊约定：每个值后面带 `[可选值范围]`，形成闭环——`config` 输出的 key 可以直接用于 `config-set`。

### Keil 多项目支持

`src/keil/mod.rs` 中的 `resolve_project_path()` 和 `resolve_path()` 是核心帮助函数。`.uvmpw` 工作区文件通过 `-p <project>` 参数指定子项目（按文件名或完整路径匹配）。不指定 `-p` 时默认选择工作区中第一个项目。解析后的 `.uvprojx` 路径是相对于工作区目录的。

`parser.rs` 中 `is_workspace_file()` / `is_project_file()` 通过文件扩展名判断类型。

### 配置系统

配置两层优先级（`src/config.rs`）：
1. `~/.embconfig.toml`（高优先级，`--global` 标志写入此处）
2. `./.embconfig.toml`（低优先级，当前目录）

`resolve_uv4()` 和 `resolve_cubemx()` 在配置不存在时自动 fallback 到搜索常见安装路径。

### 串口 Daemon 架构

`serial daemon start` 以 `--internal-daemon` 隐藏参数重新 spawn 自身进程（`CREATE_NEW_PROCESS_GROUP`），通过文件系统通信：
- `%TEMP%/emb/daemons/<id>/buffer.bin` — 接收数据缓冲区（read 后清空）
- `%TEMP%/emb/daemons/<id>/send_data` — 发送数据暂存文件
- `%TEMP%/emb/daemons/<id>/shutdown` — 关闭信号文件
- `%TEMP%/emb/daemons/<id>/history.jsonl` — 收发历史（JSONL，自动裁剪至 5MB）
- `%TEMP%/emb/daemons/<id>/meta.toml` — 元数据

`port.rs` 中 `query_wmi_port_names()` 是 Windows 独有的 WMI 友好名称查询（通过 PowerShell）。

### IOC 文件格式

`.ioc` 是 Java Properties 格式（`key=value`），key 结构为 `Category.Property.SubProperty`。`parser.rs` 按 `.` 分割提取 category 用于 `info` 命令的分组统计。`get` 命令先精确匹配，再前缀匹配。

## Skill

`skill/embedded-dev.md` 是 Claude Code 的 Skill 文件，定义了何时触发该 Skill 以及如何使用 `emb` 完成常见工作流。修改 Skill 行为时编辑此文件。

## 常用命令速查

```bash
emb --ai keil config <path> -t <target>           # 查看全部可设置配置
emb keil config-set <path> -t <target> <key> <v>  # 修改配置
emb keil build <path>                              # 增量构建
emb ioc get <path> <prefix>                        # 按前缀浏览 IOC
emb serial daemon start <port> --baud 115200       # 启动串口守护进程
```
