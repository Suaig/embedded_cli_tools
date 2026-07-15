# 安装 emb 与 keil-dev skill（AI 引导流程）

本文件给 AI 看：当 `emb --version` 跑不通（emb 没装），按本流程**引导用户**完成安装。

> **核心原则**：涉及用户系统/环境的操作——装 Rust、改 PATH、确认 Keil 安装位置、选安装目录——都必须**先询问用户、由用户确认或由用户亲手执行**。不要擅自改用户环境变量、不要假设路径。你的角色是「检测 + 解释 + 给命令让用户跑」。

仓库地址：**https://github.com/Suaig/embedded_cli_tools.git**

---

## 步骤 1：检测 Rust 工具链

emb 是 Rust，需要 cargo。先检测：

```
cargo --version
```

- 能出版本号 → 进步骤 2。
- 报错（没装）→ 引导用户装 Rust（推荐 https://rustup.rs，或 `winget install Rustlang.Rustup`）。装完让用户**重开终端**再测。**不要替用户装**，等用户确认装好。

## 步骤 2：问用户装哪 + 克隆 + 构建

**问用户**想把仓库克隆到哪个目录（给建议，让用户定）：

> 「emb 准备克隆到哪？建议 `D:\embedded_cli_tools` 或你常用的代码目录，你定。」

**不要假设路径。** 用户确认目录 `<DIR>` 后，执行（或用户授权你执行）：

```
git clone https://github.com/Suaig/embedded_cli_tools.git <DIR>
cd <DIR>
cargo build --release
```

产物：`<DIR>\target\release\emb.exe`。

## 步骤 3：把 emb 加进 PATH（让用户执行）

这步改用户环境变量，**让用户执行或明确确认**。给 PowerShell 命令：

```powershell
[Environment]::SetEnvironmentVariable("Path", $env:Path + ";<DIR>\target\release", "User")
```

- 提醒用户改完**重开终端**才生效。
- 临时验证（不改 PATH）：`$env:Path += ";<DIR>\target\release"` 后 `emb --version`。
- 用户不想改 PATH 也行——以后调用写全路径 `<DIR>\target\release\emb.exe`。

验证：`emb --version` 能出版本号。

## 步骤 4：配置 Keil 路径（按需）

emb 自动探测 UV4.exe，顺序：

1. `emb config keil_path`（~/.embconfig.toml，**最高优先级**）
2. 环境变量 `UV4_PATH`
3. 环境变量 `KEIL_PATH`
4. `C:\Keil_v5\UV4\UV4.exe`
5. `C:\Keil\UV4\UV4.exe`
6. 扫描 `C:\Program Files[(x86)]\Keil*\UV4\UV4.exe`

Keil 装在 `C:\Keil_v5` 通常零配置，直接跳到验证。若验证报「找不到 UV4」：问用户 Keil 装在哪，确认后：

```
emb config set keil_path "<用户给的路径>\UV4\UV4.exe"
emb config list
```

build / flash / debug 都读这个 config，配一次全局生效。

## 步骤 5：安装 skill 本身

skill 源在仓库 `skills/keil-dev/`。**问用户**怎么装：

- **用户有专门安装工具**（优先）：让用户用其工具指向仓库的 `skills/keil-dev/`，或打包后的 `.skill` 文件。
- **手动复制**：问装全局还是项目本地：
  - 全局（所有项目可用）：复制到 `~/.claude/skills/keil-dev/`
  - 项目本地：复制到 `<项目>/.claude/skills/keil-dev/`
  - 用户确认目标位置后再复制。

## 验证

```
emb --version
emb keil info <某 .uvprojx>     # 能出工程概览 = emb OK
emb serial scan                 # 能列出串口 = emb OK
```

skill 是否生效看安装方式：全局 skill 自动加载；项目本地在该项目生效。
