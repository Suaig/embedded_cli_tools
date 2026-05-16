---
name: embedded-dev
description: Use this skill whenever the user works with STM32/ARM embedded projects — even if they don't mention `emb` explicitly. This includes reading or modifying Keil .uvprojx/.uvmpw project files, STM32CubeMX .ioc configuration files, or serial port operations. Also triggers when the user wants to: build/compile firmware, flash firmware, change compiler settings (optimization, C standard, warnings), add or remove source files from a project, check preprocessor defines or include paths, generate code from CubeMX, read/write IOC parameters, or communicate via serial port. Triggers on keywords like: Keil, MDK, ARM-ADS, STM32, CubeMX, HAL, .uvprojx, .uvmpw, .ioc, firmware, embedded, flash, serial, COM port, uv4, ARMCLANG, ARMCC, scatter file, linker script. Use this skill even if the user just asks "what compiler does this project use" or "add a file to the build" — emb can answer and do it faster than reading raw XML.
---

# emb -- 嵌入式开发 CLI 工具

`emb` 是一个面向嵌入式开发的命令行工具，支持 Keil 项目解析/编辑/构建、STM32CubeMX IOC 文件操作、串口通信等功能。不要直接读写 .uvprojx XML 或 .ioc 文本文件 — 用 `emb` 的子命令操作，它会处理 XML 解析、路径修复、备份等细节。

所有命令支持三种输出格式：
- 默认（无 flag）：人类友好的表格格式
- `--ai`：紧凑格式，节省 token，适合 AI 读取
- `--json`：结构化 JSON，适合程序解析

`--ai` 和 `--json` 互斥，不可同时使用。

---

## Keil 项目操作

所有 keil 子命令的 `<PATH>` 参数支持 `.uvprojx`（单项目）和 `.uvmpw`（多项目工作区）。对于工作区文件，使用 `-p <project>` 指定子项目（按文件名匹配，如 `Test_Boot.uvprojx`）。不指定 `-p` 时默认选择工作区中第一个项目。

### 探索项目

```bash
# 查看项目概览（.uvprojx 单项目）
emb keil info <path>

# 查看工作区项目列表（.uvmpw）
emb keil info <workspace.uvmpw>

# 查看工作区中指定目标的详情
emb keil info <workspace.uvmpw> -t "TargetName"

# 查看工作区中指定子项目的目标详情
emb keil info <workspace.uvmpw> -t "TargetName" -p "SubProject.uvprojx"
```

### 配置查看与修改（闭环）

`keil config` 输出的每个 key 都可以直接用于 `keil config-set`，形成闭环。

```bash
# 查看所有配置项（key = value 格式，含可选值范围）
emb keil config -t "TargetName" <path> --ai

# 按类别过滤（device/output/ccompiler/asm/linker/memory）
emb keil config -t "TargetName" <path> ccompiler --ai

# 修改配置项（key 与 config 输出完全一致）
emb keil config-set -t "TargetName" <path> <key> <value>
```

**config 输出示例（--ai 模式）：**

```
device.name:STM32H750XBHx
output.name:signal_project
output.hex:yes
output.debug_info:yes
ccompiler.ac6:AC6 [yes=AC6 no=AC5]
ccompiler.pcc:6240000::V6.24::ARMCLANG (AC6 compiler, format: <id>::<version>::<tool>)
ccompiler.optim:4 (O3) [0=default 1=O0 2=O1 3=O2 4=O3 5=Ofast 6=Os 7=Oz 8=Omax]
ccompiler.otime:no
ccompiler.c99:yes
ccompiler.gnu:yes
ccompiler.wlevel:3 (High) [0=None 1=Low 2=Medium 3=High]
ccompiler.strict:no
ccompiler.one_elf:yes
ccompiler.ropi:no
ccompiler.rwpi:no
ccompiler.v6lang:3 (c99) [0=auto 1=c90 2=gnu90 3=c99 4=gnu99 5=c11 6=gnu11]
ccompiler.v6langp:3 (c++11) [0=auto 1=c++98 2=gnu++98 3=c++11 4=gnu++11 5=c++14 6=gnu++14]
ccompiler.short_enums:yes
ccompiler.short_wchar:yes
ccompiler.misc:
asm.misc:
linker.scatter:STM32H750XBHX_FLASH.ld
linker.misc:
memory.irom.start:0x08000000
memory.irom.size:0x20000
memory.iram.start:0x20000000
memory.iram.size:0x100000
memory.xram.start:0x0
memory.xram.size:0x0
```

**值格式说明：**
- bool 字段：`yes` / `no`
- 枚举字段：`当前值 (说明) [全部可选项]`，如 `4 (O3) [0=default 1=O0 ...]`
- 优化级别映射因 AC5/AC6 不同而不同（AC6 有 0-8 共 9 档）
- `ccompiler.pcc` 编译器版本格式：`<id>::<version>::<tool>`（如 `6240000::V6.24::ARMCLANG`）

**所有可设置的 key：**

| 类别 | key | 说明 |
|------|-----|------|
| device | device.name | 芯片型号 |
| output | output.name | 输出文件名 |
| output | output.hex | 生成 HEX (yes/no) |
| output | output.debug_info | 调试信息 (yes/no) |
| ccompiler | ccompiler.ac6 | AC6/AC5 选择 (yes/no) |
| ccompiler | ccompiler.pcc | 编译器版本字符串 |
| ccompiler | ccompiler.optim | 优化级别 |
| ccompiler | ccompiler.otime | 优化方向 (yes/no) |
| ccompiler | ccompiler.c99 | C99 模式 (yes/no) |
| ccompiler | ccompiler.gnu | GNU 扩展 (yes/no) |
| ccompiler | ccompiler.wlevel | 警告级别 |
| ccompiler | ccompiler.strict | 严格模式 (yes/no) |
| ccompiler | ccompiler.one_elf | 每个 function 单独 section (yes/no) |
| ccompiler | ccompiler.ropi | ROPI (yes/no) |
| ccompiler | ccompiler.rwpi | RWPI (yes/no) |
| ccompiler | ccompiler.v6lang | AC6 C 语言标准 |
| ccompiler | ccompiler.v6langp | AC6 C++ 语言标准 |
| ccompiler | ccompiler.short_enums | 短枚举 (yes/no) |
| ccompiler | ccompiler.short_wchar | 短 wchar (yes/no) |
| ccompiler | ccompiler.misc | 其他编译选项 |
| asm | asm.misc | 汇编其他选项 |
| linker | linker.scatter | scatter 文件路径 |
| linker | linker.misc | 链接其他选项 |
| memory | memory.irom.start | Flash 起始地址 |
| memory | memory.irom.size | Flash 大小 |
| memory | memory.iram.start | RAM 起始地址 |
| memory | memory.iram.size | RAM 大小 |
| memory | memory.xram.start | 外部 RAM 起始地址 |
| memory | memory.xram.size | 外部 RAM 大小 |

### 宏定义与头文件路径

```bash
# 查看预处理器宏定义
emb keil defines -t "TargetName" <path> --ai

# 添加/移除宏定义
emb keil defines-add -t "TargetName" <path> USE_HAL_DRIVER
emb keil defines-remove -t "TargetName" <path> USE_HAL_DRIVER

# 查看头文件搜索路径
emb keil includes -t "TargetName" <path> --ai

# 添加/移除头文件路径
emb keil includes-add -t "TargetName" <path> ./Drivers/CMSIS/Include
emb keil includes-remove -t "TargetName" <path> ./Drivers/CMSIS/Include
```

### 源文件管理

```bash
# 查看源文件分组
emb keil groups -t "TargetName" <path> --ai

# 查看所有源文件
emb keil files -t "TargetName" <path> --ai

# 按分组过滤
emb keil files -t "TargetName" -g "Source" <path> --ai

# 管理分组
emb keil group-add -t "TargetName" <path> "MyGroup"
emb keil group-remove -t "TargetName" <path> "MyGroup"
emb keil group-rename -t "TargetName" <path> "OldName" "NewName"

# 添加/移除源文件
emb keil file-add -t "TargetName" -g "Source" <path> ./src/main.c
emb keil file-remove -t "TargetName" -g "Source" <path> main.c

# 排除/恢复构建文件
emb keil file-exclude -t "TargetName" -g "Source" <path> test.c
emb keil file-include -t "TargetName" -g "Source" <path> test.c
```

### 构建与烧录

```bash
emb keil build <path> [-t "TargetName"]
emb keil rebuild <path> [-t "TargetName"]
emb keil clean <path> [-t "TargetName"]
emb keil flash <path> [-t "TargetName"]
```

---

## IOC 文件操作

### 浏览配置

```bash
# 查看 IOC 文件概览
emb ioc info <path>

# 按外设前缀查询参数（如查看 RCC 相关配置）
emb ioc get <path> RCC

# 查询具体参数值
emb ioc get <path> RCC.SYSCLKFreq_VALUE

# 查看引脚配置
emb ioc get <path> PA5
```

### 编辑配置

```bash
# 设置参数
emb ioc set <path> RCC.SYSCLKFreq_VALUE 72000000

# 删除参数
emb ioc rm <path> PA5.Locked
```

### 生成代码

```bash
# 调用 STM32CubeMX 生成代码（自动检测或使用配置的路径）
emb ioc generate <path>

# 指定 CubeMX 路径（优先级最高）
emb ioc generate <path> --cubemx "C:/ST/STM32CubeMX/STM32CubeMX.exe"
```

CubeMX 生成后会自动修复已知的路径 bug（绝对路径替换为相对路径、移除重复分组）。

---

## 配置管理

`emb` 使用 `.embconfig.toml` 配置文件管理工具路径。配置文件按优先级加载：
1. 用户目录 `~/.embconfig.toml`（优先级高）
2. 当前工作目录 `.embconfig.toml`（优先级低）

```bash
# 查看当前配置
emb config list

# 设置 Keil UV4 路径（保存到用户目录，全局生效）
emb config set keil_path "C:\Keil_v5\UV4\UV4.exe" --global

# 设置 STM32CubeMX 路径
emb config set cubemx_path "C:\ST\STM32CubeMX\STM32CubeMX.exe" --global

# 保存到当前目录（仅当前项目生效）
emb config set keil_path "C:\Keil_v5\UV4\UV4.exe"

# 移除配置项
emb config unset keil_path --global
```

配置项说明：
- `keil_path`：UV4.exe 的完整路径，用于 build/rebuild/clean/flash 命令
- `cubemx_path`：STM32CubeMX.exe 的完整路径，用于 ioc generate 命令

如果未配置，工具会自动检测：先检查环境变量（`UV4_PATH`/`KEIL_PATH`/`CUBEMX_PATH`），再搜索常见安装路径。

---

## 串口操作

### 扫描端口

```bash
emb serial scan
```

### 短期收发

```bash
# 发送数据（文本或 hex）
emb serial send COM3 "Hello" --baud 115200
emb serial send COM3 "AA55FF" --hex --baud 115200

# 接收数据（带超时）
emb serial recv COM3 --timeout 5000 --baud 115200
emb serial recv COM3 --timeout 3000 --hex
```

### 守护进程模式（长期监控）

```bash
# 启动守护进程
emb serial daemon start COM3 --baud 115200

# 查看运行中的守护进程
emb serial daemon list

# 通过守护进程发送数据
emb serial daemon send <id> "AT\r\n"
emb serial daemon send <id> "41540D0A" --hex

# 读取未读数据（读取后自动清空，下次只返回新数据）
emb serial daemon read <id>
emb serial daemon read <id> --hex

# 查看收发历史（带时间戳，默认最近 100 条）
emb serial daemon history <id>
emb serial daemon history <id> --limit 50
emb serial daemon history <id> --clear

# 停止守护进程
emb serial daemon stop <id>
```

---

## 常见工作流

### 1. 探索未知项目

```bash
# 单项目
emb --ai keil info project.uvprojx
emb --ai keil config -t "Target 1" project.uvprojx
emb --ai keil files -t "Target 1" project.uvprojx

# 多项目工作区
emb --ai keil info workspace.uvmpw
emb --ai keil config -t "Target 1" -p "SubProject.uvprojx" workspace.uvmpw
```

### 2. 修改编译配置

```bash
# 先查看当前配置和可选值范围
emb --ai keil config -t "Target 1" project.uvprojx ccompiler

# 根据输出中的 [可选值范围] 设置新值
emb keil config-set -t "Target 1" project.uvprojx ccompiler.optim 6
emb keil config-set -t "Target 1" project.uvprojx ccompiler.c99 yes
```

### 3. 添加新源文件

```bash
emb keil file-add -t "Target 1" -g "Source" project.uvprojx ./src/new_module.c
```

### 4. 构建并烧录

```bash
emb keil build project.uvprojx -t "Target 1"
emb keil flash project.uvprojx -t "Target 1"
```

### 5. 监控串口输出

```bash
emb serial daemon start COM3 --baud 115200
emb serial daemon read <id>
emb serial daemon stop <id>
```

---

## 输出格式选择建议

| 场景 | 推荐 flag | 说明 |
|------|-----------|------|
| AI 读取命令结果 | `--ai` | 紧凑文本，最省 token |
| 脚本解析结果 | `--json` | 结构化 JSON |
| 人类查看结果 | 无 flag | 表格格式，可读性最佳 |
