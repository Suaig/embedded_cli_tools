---
name: embedded-dev
description: Use when working with Keil projects (.uvprojx/.uvmpw), STM32CubeMX IOC files, or serial port operations. Triggers on: embedded development, STM32, Keil, CubeMX, firmware, serial port, flash, build firmware, .uvprojx, .ioc, OpenOCD.
---

# emb -- 嵌入式开发 CLI 工具

`emb` 是一个面向嵌入式开发的命令行工具，支持 Keil 项目解析/编辑/构建、STM32CubeMX IOC 文件操作、串口通信等功能。

所有命令支持三种输出格式：
- 默认（无 flag）：人类友好的表格格式
- `--ai`：紧凑格式，节省 token，适合 AI 读取
- `--json`：结构化 JSON，适合程序解析

`--ai` 和 `--json` 互斥，不可同时使用。

---

## Keil 项目操作

### 探索项目

```bash
# 查看项目概览（目标列表、芯片型号等）
emb keil info <path>

# 查看指定目标的编译器配置
emb keil config -t "Target 1" <path> ccompiler

# 查看指定目标的源文件列表
emb keil files -t "Target 1" <path>

# 查看预处理器宏定义
emb keil defines -t "Target 1" <path>

# 查看头文件搜索路径
emb keil includes -t "Target 1" <path>

# 查看源文件分组
emb keil groups -t "Target 1" <path>
```

### 修改项目

```bash
# 设置编译配置项
emb keil config set -t "Target 1" <path> <key> <value>

# 添加/移除预处理器宏
emb keil defines add -t "Target 1" <path> USE_HAL_DRIVER
emb keil defines remove -t "Target 1" <path> USE_HAL_DRIVER

# 添加/移除头文件搜索路径
emb keil includes add -t "Target 1" <path> ./Drivers/CMSIS/Include
emb keil includes remove -t "Target 1" <path> ./Drivers/CMSIS/Include

# 管理源文件分组
emb keil group add -t "Target 1" <path> "MyGroup"
emb keil group remove -t "Target 1" <path> "MyGroup"
emb keil group rename -t "Target 1" <path> "OldName" "NewName"

# 添加/移除源文件
emb keil file add -t "Target 1" -g "Source" <path> ./src/main.c
emb keil file remove -t "Target 1" -g "Source" <path> main.c

# 排除/恢复构建文件
emb keil file exclude -t "Target 1" -g "Source" <path> test.c
emb keil file include -t "Target 1" -g "Source" <path> test.c
```

### 构建与烧录

```bash
emb keil build <path> [-t "Target 1"]
emb keil rebuild <path> [-t "Target 1"]
emb keil clean <path> [-t "Target 1"]
emb keil flash <path> [-t "Target 1"]
```

### 多项目工作区

对于 `.uvmpw` 多项目工作区，使用 `-p` 参数指定项目名称：

```bash
emb keil info <workspace.uvmpw> -p "ProjectName"
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
# 调用 STM32CubeMX 生成代码
emb ioc generate <path> --cubemx "C:/ST/STM32CubeMX/STM32CubeMX.exe"
```

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
emb serial daemon send <id> "AT" --hex

# 读取守护进程缓冲区数据
emb serial daemon read <id>
emb serial daemon read <id> --timeout 2000 --hex --clear

# 停止守护进程
emb serial daemon stop <id>
```

---

## 常见工作流

### 1. 探索未知项目

```bash
emb --ai keil info project.uvprojx
emb --ai keil config -t "Target 1" project.uvprojx ccompiler
emb --ai keil files -t "Target 1" project.uvprojx
```

### 2. 添加新源文件

```bash
emb keil file add -t "Target 1" -g "Source" project.uvprojx ./src/new_module.c
```

### 3. 修改编译优化等级

```bash
emb keil config set -t "Target 1" project.uvprojx ccompiler.Optimize 2
```

### 4. 构建并烧录

```bash
emb keil build project.uvprojx -t "Target 1"
emb keil flash project.uvprojx -t "Target 1"
```

### 5. 监控串口输出

```bash
emb serial daemon start COM3 --baud 115200
emb serial daemon read <id> --timeout 10000
emb serial daemon stop <id>
```

---

## 输出格式选择建议

| 场景 | 推荐 flag | 说明 |
|------|-----------|------|
| AI 读取命令结果 | `--ai` | 紧凑文本，最省 token |
| 脚本解析结果 | `--json` | 结构化 JSON |
| 人类查看结果 | 无 flag | 表格格式，可读性最佳 |
