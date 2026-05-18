# emb 测试验收规范

## 1. 测试策略概述

| 层级 | 范围 | 触发方式 | 通过标准 |
|------|------|----------|----------|
| 单元测试 | 纯函数（protocol、parser、formatter） | `cargo test` | 全部通过 |
| 自动化集成测试 | CLI 命令 + Test_Project 文件 | `bash tests/run_tests.sh` | 全部 PASS |
| 手工验收 | 需要真实硬件的功能 | checklist | 逐项签字 |

### 测试数据

`Test_Project/` 目录包含一个真实的 STM32H7RS 多项目工作区：

| 文件 | 说明 |
|------|------|
| `Test_Project/MDK-ARM/Project.uvmpw` | Workspace，含 2 个子项目 |
| `Test_Project/MDK-ARM/Boot/Test_Boot.uvprojx` | Boot 子项目，Target `Test_Boot`，AC6 |
| `Test_Project/MDK-ARM/Appli/Test_Appli.uvprojx` | Appli 子项目，Target `Test_Appli`，AC6 |
| `Test_Project/Test.ioc` | IOC 文件，STM32H7R7I8Kx，含 RCC/USART/GPIO 等配置 |

Workspace 子项目映射：
- `.\Boot\Test_Boot.uvprojx` — 第一个子项目（默认选择）
- `.\Appli\Test_Appli.uvprojx` — 第二个子项目

---

## 2. 单元测试 (`cargo test`)

### 2.1 Serial Protocol (`src/serial/protocol.rs`)

| 编号 | 测试项 | 输入 | 预期输出 |
|------|--------|------|----------|
| UT-PROTO-01 | HEX 解码 | `decode_hex("4154")` | `[0x41, 0x54]` |
| UT-PROTO-02 | HEX 解码空字符串 | `decode_hex("")` | `[]` |
| UT-PROTO-03 | HEX 解码奇数长度 | `decode_hex("ABC")` | `Err` |
| UT-PROTO-04 | HEX 解码非法字符 | `decode_hex("GG")` | `Err` |
| UT-PROTO-05 | HEX 编码 | `encode_hex(&[0x41, 0x54])` | `"4154"` |
| UT-PROTO-06 | HEX 编码空数据 | `encode_hex(&[])` | `""` |

### 2.2 Config (`src/config.rs`)

| 编号 | 测试项 | 输入 | 预期 |
|------|--------|------|------|
| UT-CFG-01 | 设置合法 key | `set("keil_path", "C:/uv4.exe", false)` | Ok |
| UT-CFG-02 | 设置合法 key | `set("cubemx_path", "C:/cubemx.exe", false)` | Ok |
| UT-CFG-03 | 设置非法 key | `set("invalid_key", "val", false)` | `Err` |
| UT-CFG-04 | 移除已存在 key | `unset("keil_path", false)` | Ok |
| UT-CFG-05 | 移除不存在的配置 | 无 `.embconfig.toml` 时 unset | `Err` |
| UT-CFG-06 | global 写入用户目录 | `set("keil_path", "...", true)` | Ok |
| UT-CFG-07 | 合并优先级 | 用户配置 + 项目配置 | 用户配置覆盖项目配置 |

### 2.3 IOC Parser (`src/ioc/parser.rs`)

| 编号 | 测试项 | 输入 | 预期 |
|------|--------|------|------|
| UT-IOC-01 | 解析 IOC 文件 | 合法 `.ioc` | `IocFile` 含 categories 和 entries |
| UT-IOC-02 | 解析空文件 | 空文件 | 空 categories/entries |
| UT-IOC-03 | 注释行 | `# comment\nkey=value` | 注释忽略，`key=value` 解析 |
| UT-IOC-04 | 精确 key 匹配 | `ioc.get("RCC.SYSCLKFreq_VALUE")` | 返回单个值 |
| UT-IOC-05 | 前缀匹配 | `ioc.get_by_prefix("RCC")` | 所有 `RCC.*` 键值对 |
| UT-IOC-06 | 不存在的 key | `ioc.get("NONEXIST")` | None |
| UT-IOC-07 | 不存在的 prefix | `ioc.get_by_prefix("NONEXIST")` | 空 Vec |

### 2.4 Keil Parser (`src/keil/parser.rs`)

| 编号 | 测试项 | 输入 | 预期 |
|------|--------|------|------|
| UT-KEIL-01 | 解析 `.uvprojx` | 合法单项目 XML | `KeilProject` 含 targets |
| UT-KEIL-02 | 解析 `.uvmpw` | 合法 workspace XML | `KeilWorkspace` 含 projects |
| UT-KEIL-03 | 文件类型识别 | `.uvprojx` | `is_project_file` = true |
| UT-KEIL-04 | workspace 识别 | `.uvmpw` | `is_workspace_file` = true |
| UT-KEIL-05 | 不支持的文件类型 | `.txt` | 报错 |
| UT-KEIL-06 | 错误 XML | 非法 XML | parse error |

### 2.5 Output Formatter (`src/output/`)

| 编号 | 测试项 | 输入 | 预期 |
|------|--------|------|------|
| UT-OUT-01 | Human Table | `OutputValue::Table {...}` | ASCII 表格 |
| UT-OUT-02 | Human KeyValue | `OutputValue::KeyValue(...)` | `key:value` |
| UT-OUT-03 | Human List | `OutputValue::List(...)` | 每行一个 |
| UT-OUT-04 | Human Message | `OutputValue::Message("ok")` | `ok` |
| UT-OUT-05 | AI Table | 同上 Table | 无边框紧凑 |
| UT-OUT-06 | JSON Table | 同上 Table | 合法 JSON |
| UT-OUT-07 | JSON KeyValue | 同上 KeyValue | 合法 JSON |

---

## 3. 自动化集成测试 (`bash tests/run_tests.sh`)

使用 Test_Project 真实文件，所有路径相对于项目根目录。

### 3.1 CLI 入口

| 编号 | 命令 | 预期退出码 | 预期输出 |
|------|------|------------|----------|
| IT-CLI-01 | `emb` | ≠0 | `no command specified` |
| IT-CLI-02 | `emb --ai --json keil info Test_Project/MDK-ARM/Appli/Test_Appli.uvprojx` | ≠0 | `--ai and --json are mutually exclusive` |
| IT-CLI-03 | `emb --version` | 0 | `emb` + 版本号 |
| IT-CLI-04 | `emb --help` | 0 | 含 `keil`/`ioc`/`serial`/`debug`/`config` |

### 3.2 错误处理

| 编号 | 命令 | 预期退出码 | 预期输出 |
|------|------|------------|----------|
| IT-ERR-01 | `emb keil info nonexistent.uvprojx` | ≠0 | 文件不存在 |
| IT-ERR-02 | `emb keil info not_project.txt` | ≠0 | `unsupported file type` |
| IT-ERR-03 | `emb keil info Test_Project/MDK-ARM/Appli/Test_Appli.uvprojx -t "NoTarget"` | ≠0 | `target 'NoTarget' not found` |
| IT-ERR-04 | `emb keil info Test_Project/MDK-ARM/Project.uvmpw -p "NoProj.uvprojx"` | ≠0 | `project 'NoProj.uvprojx' not found` |
| IT-ERR-05 | `emb ioc get Test_Project/Test.ioc NONEXIST` | ≠0 | `No entries found matching prefix` |
| IT-ERR-06 | `emb config set bad_key value` | ≠0 | `unknown config key` |

### 3.3 Keil — Info

| 编号 | 命令 | 预期退出码 | 关键验证 |
|------|------|------------|----------|
| IT-KEIL-INFO-01 | `emb keil info Test_Project/MDK-ARM/Project.uvmpw` | 0 | 表格含 `Test_Boot.uvprojx`、`Test_Appli.uvprojx`，Active 列 |
| IT-KEIL-INFO-02 | `emb --ai keil info Test_Project/MDK-ARM/Boot/Test_Boot.uvprojx` | 0 | 表格含 `Test_Boot`，Device 列 |
| IT-KEIL-INFO-03 | `emb --ai keil info Test_Project/MDK-ARM/Boot/Test_Boot.uvprojx -t "Test_Boot"` | 0 | `Target:Test_Boot`、`Device:STM32H7R7I8Kx`、`AC6:yes` |
| IT-KEIL-INFO-04 | `emb --ai keil info Test_Project/MDK-ARM/Appli/Test_Appli.uvprojx -t "Test_Appli"` | 0 | `Target:Test_Appli`、`Device:STM32H7R7I8Kx` |
| IT-KEIL-INFO-05 | `emb --ai keil info Test_Project/MDK-ARM/Project.uvmpw -t "Test_Boot"` | 0 | 自动选第一个子项目，`Target:Test_Boot` |
| IT-KEIL-INFO-06 | `emb --ai keil info Test_Project/MDK-ARM/Project.uvmpw -t "Test_Appli" -p "Test_Appli.uvprojx"` | 0 | `Target:Test_Appli` |
| IT-KEIL-INFO-07 | `emb --json keil info Test_Project/MDK-ARM/Appli/Test_Appli.uvprojx` | 0 | 合法 JSON，含 `headers`/`rows`，`rows[0]` 含 `Test_Appli` |

### 3.4 Keil — Config

| 编号 | 命令 | 预期退出码 | 关键验证 |
|------|------|------------|----------|
| IT-KEIL-CFG-01 | `emb --ai keil config Test_Project/MDK-ARM/Appli/Test_Appli.uvprojx -t "Test_Appli"` | 0 | 含 `device.name:STM32H7R7I8Kx`、`ccompiler.ac6:AC6`、`ccompiler.optim` 含 `[0=default...]` |
| IT-KEIL-CFG-02 | `emb --ai keil config Test_Project/MDK-ARM/Appli/Test_Appli.uvprojx -t "Test_Appli" ccompiler` | 0 | 仅 `ccompiler.*` 行，不出现 `device.`/`linker.` |
| IT-KEIL-CFG-03 | `emb --ai keil config Test_Project/MDK-ARM/Project.uvmpw -t "Test_Boot" -p "Test_Boot.uvprojx" ccompiler` | 0 | 子项目 Boot 的 ccompiler 配置 |

#### Config 闭环修改（需还原）

| 编号 | 步骤 | 预期 |
|------|------|------|
| IT-KEIL-CFG-04 | `config-set ... ccompiler.optim 2` → `config` 确认 | 值变为 `2 (O1)` |
| IT-KEIL-CFG-05 | `config-set ... ccompiler.optim 4` 还原 | 值恢复 `4 (O3)` |
| IT-KEIL-CFG-06 | `config-set ... ccompiler.c99 no` → `config` 确认 | `ccompiler.c99:no` |
| IT-KEIL-CFG-07 | `config-set ... ccompiler.c99 yes` 还原 | `ccompiler.c99:yes` |
| IT-KEIL-CFG-08 | `config-set ... output.hex maybe` | ≠0，报错 |

### 3.5 Keil — Defines / Includes / Groups / Files

| 编号 | 命令 | 预期退出码 | 关键验证 |
|------|------|------------|----------|
| IT-KEIL-DEF-01 | `emb --ai keil defines Test_Project/MDK-ARM/Appli/Test_Appli.uvprojx -t "Test_Appli"` | 0 | 含 `USE_HAL_DRIVER`、`STM32H7R7xx` |
| IT-KEIL-DEF-02 | `emb --ai keil defines Test_Project/MDK-ARM/Project.uvmpw -t "Test_Appli" -p "Test_Appli.uvprojx"` | 0 | 与上一项结果一致 |
| IT-KEIL-INC-01 | `emb --ai keil includes Test_Project/MDK-ARM/Appli/Test_Appli.uvprojx -t "Test_Appli"` | 0 | 含 `Drivers/` 路径 |
| IT-KEIL-GRP-01 | `emb keil groups Test_Project/MDK-ARM/Appli/Test_Appli.uvprojx -t "Test_Appli"` | 0 | 含 `Application/`、`Drivers/` 组 |
| IT-KEIL-GRP-02 | `emb keil files Test_Project/MDK-ARM/Appli/Test_Appli.uvprojx -t "Test_Appli"` | 0 | 含 `main.c`、`stm32h7rsxx_it.c`，Status 列 |

#### Defines 增删（需还原）

| 编号 | 步骤 | 预期 |
|------|------|------|
| IT-KEIL-DEF-03 | `defines-add TEST_MACRO` → `defines` 确认 | `TEST_MACRO` 出现在列表中 |
| IT-KEIL-DEF-04 | `defines-remove TEST_MACRO` → `defines` 确认 | `TEST_MACRO` 不在列表中 |

#### Includes 增删（需还原）

| 编号 | 步骤 | 预期 |
|------|------|------|
| IT-KEIL-INC-02 | `includes-add "./TestInc"` → `includes` 确认 | `.\TestInc` 出现 |
| IT-KEIL-INC-03 | `includes-remove "./TestInc"` → `includes` 确认 | `.\TestInc` 消失 |

### 3.6 IOC 模块

| 编号 | 命令 | 预期退出码 | 关键验证 |
|------|------|------------|----------|
| IT-IOC-01 | `emb ioc info Test_Project/Test.ioc` | 0 | 含 `Mcu`、`RCC`、`NVIC1`、`ProjectManager`，Entries 列 > 0 |
| IT-IOC-02 | `emb ioc get Test_Project/Test.ioc RCC` | 0 | 含 `RCC.SYSCLKFreq_VALUE` |
| IT-IOC-03 | `emb ioc get Test_Project/Test.ioc ProjectManager` | 0 | 含 `ProjectManager.HeapSize` |
| IT-IOC-04 | `emb ioc get Test_Project/Test.ioc Mcu.Name` | 0 | `STM32H7R7I8Kx` |
| IT-IOC-05 | `emb ioc get Test_Project/Test.ioc NONEXIST` | ≠0 | `No entries found matching prefix: NONEXIST` |

#### IOC 编辑（需还原）

| 编号 | 步骤 | 预期 |
|------|------|------|
| IT-IOC-06 | `ioc set Test.ioc TEST.Key 12345` → `ioc get Test.ioc TEST` | `TEST.Key:12345` |
| IT-IOC-07 | `ioc rm Test.ioc TEST.Key` → `ioc get Test.ioc TEST` | ≠0（不存在） |

### 3.7 Serial 模块（不需要真实串口）

| 编号 | 命令 | 预期退出码 | 关键验证 |
|------|------|------------|----------|
| IT-SER-01 | `emb serial scan` | 0 | 输出表格（含 Port, Name 列）或 "No serial ports found" |
| IT-SER-02 | `emb serial send COM99 "test"` | ≠0 | 端口不存在错误 |

### 3.8 Config 模块

| 编号 | 命令 | 预期退出码 | 关键验证 |
|------|------|------------|----------|
| IT-CFG-01 | `emb config list` | 0 | 含 `keil_path`、`cubemx_path` 行 |

### 3.9 Debug 模块

| 编号 | 命令 | 预期退出码 | 关键验证 |
|------|------|------------|----------|
| IT-DBG-01 | `emb debug openocd -f test.cfg` | 0 | `not yet implemented` |
| IT-DBG-02 | `emb debug keil test.uvprojx` | 0 | `not yet implemented` |

### 3.10 输出格式

| 编号 | 命令 | 预期退出码 | 关键验证 |
|------|------|------------|----------|
| IT-FMT-01 | `emb --json keil info Test_Project/MDK-ARM/Appli/Test_Appli.uvprojx` | 0 | `jq .` 可解析，`headers`/`rows` 存在 |
| IT-FMT-02 | `emb --json ioc get Test_Project/Test.ioc Mcu.Name` | 0 | `jq .` 可解析 |
| IT-FMT-03 | `emb --ai keil config ... ccompiler` | 0 | 每行 `key:value [...或]` 格式 |
| IT-FMT-04 | `emb keil files ...` | 0 | comfy-table 框线 |

### 3.11 Workspace 一致性

| 编号 | 验证 |
|------|------|
| IT-WS-01 | `emb --ai keil info <Test_Boot.uvprojx> -t "Test_Boot"` 与 `emb --ai keil info <Project.uvmpw> -t "Test_Boot"` 输出一致 |
| IT-WS-02 | `emb --ai keil config <Test_Appli.uvprojx> -t "Test_Appli"` 与 `emb --ai keil config <Project.uvmpw> -t "Test_Appli" -p "Test_Appli.uvprojx"` 输出一致 |

---

## 4. 手工验收清单

需要真实硬件（STM32 开发板 + DAP-Link/ST-Link + Keil MDK + CubeMX）。

### 4.1 Keil 构建

- [ ] **KEIL-HW-01** `emb keil build Test_Project/MDK-ARM/Appli/Test_Appli.uvprojx -t "Test_Appli"` 编译成功
- [ ] **KEIL-HW-02** `emb keil rebuild ...` 先 clean 后 build
- [ ] **KEIL-HW-03** `emb keil flash ...` 下载到开发板
- [ ] **KEIL-HW-04** 修改优化级别后 build 生效

### 4.2 IOC 生成

- [ ] **IOC-HW-01** `emb ioc generate Test_Project/Test.ioc` 调用 CubeMX 生成成功
- [ ] **IOC-HW-02** 生成后路径修复正确

### 4.3 串口通信

- [ ] **SER-HW-01** `emb serial scan` 发现 ST-Link 虚拟串口并显示 WMI 友好名称
- [ ] **SER-HW-02** `emb serial send` 数据正确
- [ ] **SER-HW-03** `emb serial recv` 数据正确
- [ ] **SER-HW-04** daemon 模式长时间运行稳定
- [ ] **SER-HW-05** daemon 历史记录与 HEX 显示一致
- [ ] **SER-HW-06** HEX 边界值（0x00, 0xFF）传输正确

---

## 5. 回归检查清单

每次发布前：

```
cargo test                    # 全部单元测试通过
cargo build --release         # Release 无编译错误/警告
cargo clippy                  # Lint 无新增警告
bash tests/run_tests.sh       # 自动化集成测试全部通过
```

- [ ] 所有单元测试通过
- [ ] Release 构建无错误
- [ ] 自动化集成测试全部 PASS
- [ ] `emb --help` 输出正确
- [ ] `--ai` 模式 config 每个 key 后有 `[可选值范围]`
- [ ] `--json` 输出为合法 JSON
- [ ] Workspace 多项目模式正常
- [ ] 边界情况报错合理（非 panic）
