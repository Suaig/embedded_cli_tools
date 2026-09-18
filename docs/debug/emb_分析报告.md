# emb 工具对 Fan_simulator_mdk 工程的实测报告
> 用真实 STM32H7RS 双工程 Keil workspace 验证 `emb` 的解析正确性、功能完整性与健壮性，并给出缺陷清单。
**更新**: 2026-09-18 · **关联**: `src/keil/*`、`src/ioc/*`、`src/serial/*`、`tests/*.rs`、`docs/TEST_SPEC.md` · **状态**: 定稿

---

## 1. 测试对象与环境

| 项目 | 值 |
|---|---|
| 被测工具 | `embedded_cli_tools` @ `f11b522`，二进制 `emb 0.1.0`（`cargo build --release`） |
| 测试工程 | `E:\2.other_work\LW\Fan_simulator_LW\Fan_simulator_mdk`（670 MB，git 工作区有未提交改动 → 全程只读；写测试在副本上进行） |
| 工程规模 | workspace 2 子项目；`Fan_simulator_Appli.uvprojx` 516 KB / 2 target / 26 组 / 296 文件；`Fan_simulator_Boot.uvprojx` 73 KB / 1 target / 7 组 / 47 文件；`Fan_simulator.ioc` 1088 键 / 151 分类 |
| Keil MDK | V5.41，`C:\Users\<u>\AppData\Local\Keil_v5\UV4\UV4.exe` |
| STM32CubeMX | `C:\Users\<u>\AppData\Local\Programs\STM32CubeMX\STM32CubeMX.exe` |
| 未覆盖 | 烧录（需真实硬件，按要求跳过）；`debug keil` 批处理调试（需连接调试器） |

## 2. 测试方法

不依赖"输出看起来对"，用**独立基准对拍**：

1. `baseline.py` —— 只用 Python 标准库 `xml.etree` / 手写 Properties 解析，独立解析同一批 `.uvprojx` / `.uvmpw` / `.ioc`，产出 ground truth。
2. `compare_keil.py` / `compare_ioc.py` —— 逐字段比对 `emb --json` 输出与 ground truth。
3. `test_keil_write.py` —— 在副本上执行全部写命令，用 ground truth 校验写入结果，并测量 XML 格式保真度。
4. 交叉验证 —— `keil map` 的结果用 **map 文件自身的恒等式**（`Total RO = Code + RO Data` 等）与 **UV4 原生日志 `Program Size:` 行** 双重校验。

证据脚本与日志：`C:\Users\Z004N7WV\AppData\Local\Temp\emb_test\`（`fan_copy/` 为工程副本，222 MB）。

## 3. 结果总览

以下为**首次全面测试（修复前）**的原始统计，保留作为基线：

| 测试集 | 检查项 | 通过 | 失败 |
|---|---|---|---|
| `cargo test` 单元测试 | 59 | **59** | 0 |
| Keil 读命令对拍 | 43 | 42 | 1 |
| IOC 对拍 | 17 | 16 | 1 |
| Keil 写命令语义 | 36 | 30 | 6 |
| 错误路径 / 边界 | 17 | 17 | 0 |
| `keil build` / `keil map` / `locate` | 12 | 9 | 3 |
| `tests/run_tests.sh` | 26 | 11 | 15（脚本中途中止） |

**结论**：读路径的**正确性优秀**——Keil/IOC 的解析结果与独立基准逐字段一致，`locate` 行号精确，`keil build` 的数字与 UV4 原生输出逐字节吻合。主要问题集中在**失败语义**（该报错的不报错、该非零的返回 0）、**`keil map` 的列映射错误**、**Keil 写操作破坏 XML 格式**，以及**工具/工程路径自动发现覆盖不足**。

P0 与 P1 共 5 项（D1、D2、D4、D11 及 D5 的 3 个子项）已于同日修复并回归，见 §8。

## 4. 验证通过项（实测确认）

| 项 | 证据 |
|---|---|
| workspace 解析 | `.\Appli\...uvprojx`（Active=no）、`.\Boot\...uvprojx`（Active=yes、Batch=yes），与 XML 逐字段一致 |
| 工程/Target 解析 | 3 个 target 的 Device / Vendor / PackID / Cpu / SVD / uAC6 / pCCUsed / ToolsetNumber 全对 |
| `defines` | SlotA/SlotB 各 3 个宏（含 `CONFIG_MODBUS_UART=4`）、Boot 2 个，逐条一致 |
| `includes` | 45 / 33 / 8 条，顺序与内容完全一致 |
| `groups` / `files` | 组名序列、每组文件数、296 与 47 条文件记录的名称/类型/路径/排除状态全对 |
| `config` | 13 个标量字段（device.name、output.*、ccompiler.c99/gnu/one_elf/...、linker.scatter/misc）与 XML 一致 |
| `memory.*` | `IROM`/`IRAM`/`XRAM` 与 XML 对应节点一致 |
| workspace `-p` | 按文件名与完整路径 `.\Boot\Fan_simulator_Boot.uvprojx` 均可匹配；错误消息列出可选项 |
| IOC 解析 | 1088 键、151 分类计数全对；`\:` 转义正确反转义（`Boot\:Boot Project\:true` → `Boot:Boot Project:true`） |
| IOC 查询 | `RCC`(107)/`Mcu`(143)/`ProjectManager`(33)/`NVIC1`(11) 等键值逐条一致；29 个含特殊字符值全部往返正确 |
| IOC 编辑 | `set`/`rm` 后文件**字节级不变**（行级编辑），转义写回正确（`a:b=c` → `a\:b=c`） |
| `keil locate` | SlotA `defines`→341、SlotB→10102、`includes`→343/10104、`Optim`→317，与原文行号逐个吻合，且 target 感知 |
| `keil build` | Appli SlotA 构建成功；输出 `Code=40828 RO-data=5408 RW-data=40 ZI-data=14860` 与 UV4 原生 `Program Size:` 行**逐字节一致** |
| `keil map` 总计/区域 | Entry Point、Total RO/RW/ROM、执行区域表（名称/Exec/Load/Size/Max）与 map 文本完全一致 |
| 错误路径 | 不存在的文件、错误扩展名、非法 XML、无 `<Target>`、错误 target、错误子项目、错误前缀、`--ai --json` 互斥、无子命令 —— 全部 exit≠0、消息清晰、无 panic |
| 性能 | 516 KB 工程 `info`/`files` 约 150–170 ms |

## 5. 缺陷清单

### D1 [高] 构建失败时退出码仍为 0 —— 已修复

- **问题表象**：`emb keil build <proj> -t NoSuchTarget` → 表格显示 `Success: no`、`Errors: 0`、`Warnings: 0`、`Build Time` 空，**退出码 0**；`--json` 同样是 `[["Success","no"],["Errors","0"],...]`，退出码 0。
- **影响**：脚本与 AI Agent 用退出码判断构建成败会**误判为成功**。`rebuild`/`clean`/`flash` 走同一路径，同样受影响。
- **根因**：`src/keil/mod.rs` `cmd_build` 把 target 直接交给 UV4，未先走其他子命令通用的 `resolve_target()` 校验；函数末尾无视 `result.success`，无条件 `Ok(())`；日志里没有 `Error(s)` 行时 `errors` 保持 0，于是"失败"没有任何可读原因。
- **解决方案**：
  1. 调用 UV4 前先 `load_project()` + `resolve_target()` 校验 target（仅当显式传入 `-t` 时）；
  2. 抽出 `ensure_build_succeeded()`，`result.success == false` 时返回 `Err`，消息含错误/警告计数、是否产出 output 文件，以及 UV4 日志末尾 5 行。
- **验证**（2026-09-18）：
  | 场景 | 修复前 | 修复后 |
  |---|---|---|
  | 真实工程 + 不存在的 target | `Success: no`，exit 0 | `Error: target 'NoSuchTarget' not found (available: Fan_simulator_Boot)`，**exit 1** |
  | workspace + 非首个子项目的 target | 静默 `Success: no`，exit 0 | `Error: target 'Fan_simulator_Boot' not found (available: Fan_simulator_Appli_SlotA, ...)`，**exit 1** |
  | 真实编译失败（`startup_stm32h7r7xx.s` 缺失） | exit 0 | `Error: build failed: 2 error(s), 0 warning(s)` + UV4 日志尾部，**exit 1** |
  | 正常构建成功 | exit 0 | `Success: yes`，exit 0（未回归） |
  | `clean`/`rebuild`/`flash` 错误 target | exit 0 | exit 1 |

  自动化回归：新增 `tests/cli_build_exit_code.rs`（4 条，验证 target 校验且无需安装 UV4）+ `src/keil/mod.rs` 单测 2 条。全部先观察失败再实现。

### D2 [高] `keil map` 的 Grand Totals 列整体错位一位 —— 已修复

- **问题表象**：对同一份 Boot 工程的 map，`keil map` 报 `RO Data=4176  RW Data=5408  ZI Data=40`；`keil build` 与 UV4 原生日志均报 `RO-data=5408  RW-data=40  ZI-data=14860`。同一工具两条命令互相矛盾。
- **根因**：armlink 的 Grand Totals 行有 6 个数字，表头 `Code (inc. data)` 覆盖前两个数字（Code 与该 Code 内联的数据量）。`src/keil/map.rs` 把 `nums[1]` 当作 RO Data，导致 `ro_data`/`rw_data`/`zi_data` 全部前移一位，真正的 ZI Data 落进了未展示的 `debug` 字段。列位右对齐可证：

  ```
  HEADER: '      Code (inc. data)   RO Data    RW Data    ZI Data      Debug   '
  ROW   : '     40828       4176       5408         40      14860     415344   Grand Totals'
  ```

  `5408` 右对齐到 `RO Data`(列 31)、`40` 到 `RW Data`(42)、`14860` 到 `ZI Data`(53)。
- **解决方案**：`info.totals.code = nums[0]`、忽略 `nums[1]`、`ro_data = nums[2]`、`rw_data = nums[3]`、`zi_data = nums[4]`、`debug = nums[5]`；同时删除恒无来源的 `grand` 字段（它原本取自被误读的列，且全仓库无引用），并重写字段注释说明列对齐规则。
- **验证**（2026-09-18）：三份真实 map 全部通过 —— 用 armlink 自己发布的恒等式校验，并与 map 原始行的列位逐字比对。
  | map | Code | RO Data | RW Data | ZI Data | Total RO | Total RW |
  |---|---|---|---|---|---|---|
  | Appli_SlotA | 552912 | 74804 | 33767584 | 43564 | 627716 ✓ | 33811148 ✓ |
  | Appli_SlotB | 552832 | 74804 | 33767584 | 43564 | 627636 ✓ | 33811148 ✓ |
  | Boot | 40828 | 5408 | 40 | 14860 | 46236 ✓ | 14900 ✓ |

  **验证中发现的额外事实**：`Total ROM = Code + RO Data + RW Data` 中 armlink 用的是 **ROM Totals 行**的 RW Data，而非 Grand Totals 行的。SlotA/SlotB 这类 RW 数据被压缩进 ROM 的镜像两者相差数十 MB（33767584 vs 265220），该式只有在未压缩镜像上才等价。因此该式不作为普适断言写入测试，改为单独一条测试钉住"`rom_size` 取自 map 自带的 Total ROM 行、不得由前三项相加得出"。

  自动化回归：`src/keil/map.rs` 新增 2 条单测（`grand_totals_columns_satisfy_armlink_identities`、`total_rom_is_read_from_the_map_not_computed`），均先观察失败再实现；并修正了原 `parses_totals_and_entry` 中固化错误映射的断言。

### D3 [高] Keil 写操作全量重写 XML，破坏文件格式

- **问题表象**：对同一宏 `defines-add` 再 `defines-remove`（净零改动）后，**1836 行中 1829 行发生变化**，文件 72932 → 70533 字节：

  | 破坏点 | 原始 | 重写后 |
  |---|---|---|
  | 命名空间属性 | `xsi:noNamespaceSchemaLocation="..."` | `noNamespaceSchemaLocation="..."`（**前缀丢失**） |
  | 空元素 | `<MiscControls></MiscControls>` | `<MiscControls />` |
  | 空行 | 保留 | 全部删除 |
  | 属性顺序 | `Cclass= Cgroup= Cvendor= ...` | 重排为字母序 |
  | 文件结尾 | 有换行 | 无换行 |
  | XML 声明 | `standalone="no"` | 丢失 |

- **影响**：任何 `config-set` / `*-add` / `*-remove` 都让 git diff 变成整文件，code review 无法定位真实改动；`xsi:` 前缀丢失使 schema 位置对标准 XML 解析器失效。UV4 实测能容忍（对重写后的文件 `-b` 返回 0 error），因此不是数据损坏，但**不能称为"格式无损"**，与 README 中"不破坏 XML 格式"的定位不符。
- **缓解现状**：会生成 `.bak`（覆盖式，不累积）；`locate` + Read + Edit 工作流可完全绕开该问题。
- **根因**：`src/keil/editor.rs` 用 xmltree 全量解析后再序列化，未做保格式编辑或最小化 diff。
- **解决方案**：短期在文档/`--ai` 提示中明确"写操作会重写整个 XML"；长期改为按 `locate` 得到的行区间做文本级替换，或序列化后恢复原缩进/空行并按原文属性顺序输出。
- **验证**：`test_keil_write.py` 的 no-op 往返检查（`defines-add` 后 `defines-remove` 同一宏，净零改动）长期失败于"1836 行中 1829 行变化"，即为本项的常驻回归探针；修复后该检查应转为通过。

### D4 [中] UV4.exe / CubeMX 自动探测覆盖不足 —— 已修复

- **问题表象**：本机 Keil 与 CubeMX 均已安装，但 `emb debug keil` / `keil build` 报 `UV4.exe not found`，`ioc generate` 报 `STM32CubeMX not found`，必须先手工 `config set`。
- **根因**：
  - `find_uv4()` 只查 `UV4_PATH`、`KEIL_PATH`、`C:\Keil_v5`、`C:\Keil`、`C:\Program Files*` 下的 Keil 目录 —— 不查注册表，也不查 `%LOCALAPPDATA%`。Keil MDK 5.41 的较新安装器默认装到 `%LOCALAPPDATA%\Keil_v5`。
  - `find_cubemx()` 只查 `CUBEMX_PATH` 与 2 个硬编码路径，不含 `%LOCALAPPDATA%\Programs\STM32CubeMX`。
- **解决方案**：
  - `find_uv4()` 新增第 3 顺位的注册表查询（`HKLM\SOFTWARE\WOW6432Node\Keil\Products\MDK` 与 `HKLM\SOFTWARE\Keil\Products\MDK` 的 `Path` 值，经 `reg query` 读取，不加新依赖）；第 5 顺位新增 `%LOCALAPPDATA%\Keil_v5\UV4\UV4.exe` 与 `%LOCALAPPDATA%\Keil\UV4\UV4.exe`。注册表值指向 `...\Keil_v5\ARM`，UV4 取其同级目录。
  - `find_cubemx()` 新增 `%LOCALAPPDATA%\Programs\STM32CubeMX\STM32CubeMX.exe` 与 `%LOCALAPPDATA%\STM32CubeMX\STM32CubeMX.exe`。
- **验证**（2026-09-18）：在无任何 `.embconfig.toml`、无 `UV4_PATH`/`KEIL_PATH`/`CUBEMX_PATH` 的目录下 ——
  - `emb debug keil nosuch.uvprojx` 不再报 `UV4.exe not found`，改为报文件不存在（说明 UV4 已自动找到）；
  - `emb ioc generate nosuch.ioc` 同样越过 CubeMX 探测。
  - 新增 3 条单测：`reg query` 输出解析（用真实输出样本）、注册表 miss 返回 None、`ARM` 目录反推 UV4 路径。

### D5 [中] `ioc generate` 脚本模式下会卡在 CubeMX 弹窗并超时 —— 部分修复

- **问题表象**：`emb ioc generate <工程根>/Fan_simulator.ioc` 等待 120 s 后报 `Error: STM32CubeMX generation timed out`，退出码 1，`MDK-ARM/Appli`、`MDK-ARM/Boot` 一个都没更新。
- **根因**（用户确认）：CubeMX 会弹出确认对话框询问是否生成，脚本模式下无人应答 → 工具侧 120 s 超时。**这是 CubeMX 的交互行为，工具本身无法绕过**，属工具的能力边界，需要在文档中明确"`ioc generate` 需人工在弹窗点确认"。
- **解决方案**（已修复的子项）：
  1. **裸相对文件名必失败**：`emb ioc generate rel.ioc` 原报 `os error 123`（`Path::new("rel.ioc").parent()` 为 `""`，`Command::current_dir("")` 在 Windows 上失败）。新增 `project_dir_of()`，父目录为空时回退 `.`。
  2. **超时提示误导**：原提示只说"Check ~/.stm32cubemx/STM32CubeMX.log"，而日志里只有 `IP not ready for code generation: LWIP`，看不出真实原因。现提示会说明"可能正在等待 CubeMX 弹窗确认"，并**报出该次运行对应的 CubeMX 进程 PID 与可直接粘贴的停止命令**。临时脚本名加入 uuid，确保 PID 定位只命中本次运行、不会指向无关的 CubeMX 会话。
  3. **不检查输入文件是否存在**：原实现会为不存在的 .ioc 拉起 CubeMX（约 1.6 GB JVM），让它在内部抛 `NullPointerException`，再干等 120 s 并给出误导性的"弹窗"提示。现改为启动前先校验，立即返回 `ioc file not found: <path>`。
- **未修复 / 设计取舍**：
  - 超时后**不自动 kill** CubeMX 进程。自动终止看似更干净，但超时窗口恰好覆盖"用户正要点确认"的场景——杀掉会连带丢弃用户即将完成的生成。改为报告 PID + 停止命令，把决定权交回用户。如需改成自动终止，请提出。
  - 完成判定写死为 `<ioc 所在目录>/MDK-ARM/<ioc 文件名>.uvprojx`。本工程 ioc 在工程根、Keil 工程在 `MDK-ARM/Appli|Boot/` 子目录，若 CubeMX 确实按 `ProjectStructure` 写回子目录，则该判定路径永不成立。**仍需在"有人点确认"的前提下复测才能定论**（本次日志停在 `IP not ready for code generation: LWIP`，看不出目标路径）。
  - 脚本把 `project name` 固定为 ioc 文件名（`Fan_simulator`），而工程实际子项目名是 `Fan_simulator_Appli` / `Fan_simulator_Boot`，是否会另生成孤立工程同样待复测。
- **验证**（2026-09-18）：`emb ioc generate nosuch.ioc`（无任何配置）→ `Error: ioc file not found: nosuch.ioc`，exit 1，166 ms 返回；新增 `tests/cli_ioc_generate.rs` 1 条 CLI 测试（断言快速失败、不启动 CubeMX）。`current_dir` 与 PID 定位各新增单测。
- **测试披露**：`tests/cli_ioc_generate.rs` 未单独跑一次 RED。同一场景（`emb ioc generate nosuch.ioc`）在修复前已实测观察到"启动 CubeMX → 120 s 超时 → 提示未提及文件不存在"的完整行为，即该测试要断言的失败现象；重跑只会再造一个 1.6 GB JVM 而无新增信息。

### D6 [中] workspace 下 `build`/`rebuild`/`clean`/`flash`/`locate` 不支持 `-p` —— 已修复

- **问题表象**：`emb keil build <ws> -p <proj>` → `error: unexpected argument '-p' found`（exit 2）。`emb keil build <ws> -t "Fan_simulator_Boot"` 会静默产出 `Success: no`——因为 `cmd_build` 忽略子项目筛选、固定取 workspace 第一个子项目（Appli），而 Appli 里没有该 target。
- **影响**：无法从 workspace 构建 Appli 以外的子工程；与 README「所有 keil 子命令支持 workspace 多项目」的表述不符（`info`/`config`/`defines` 等读命令确实支持）。
- **根因**：`cmd_build` 签名不含 `project` 参数，内部 `resolve_project_path(path_str, &None)` 写死为 `None`；`main.rs` 的 `Build`/`Rebuild`/`Clean`/`Flash`/`Locate` 变体未定义 `-p`。
- **解决方案**：5 个子命令均补 `-p/--project`；`cmd_build` 改为走与其他子命令相同的 `resolve_path(path_str, project)`（顺带删掉了 `cmd_build` 里那段与 `resolve_path` 逐字重复的路径解析）；`locate` 改为先解析再读取。
- **验证**（2026-09-18）：
  - 新增 `tests/cli_workspace_project.rs` 4 条 CLI 测试。用两个子项目（`TargetA`/`TargetB`、`DEFINE_A`/`DEFINE_B`）证明 `-p` 真的改变了路由：不带 `-p` 时错误消息列出 `TargetA`，带 `-p b.uvprojx` 时列出 `TargetB`。
  - 真实工程：`emb keil build <Project.uvmpw> -p Fan_simulator_Boot.uvprojx -t NoSuchTarget` → 列出 `Fan_simulator_Boot`（此前是 `Success: no` 静默失败）；`emb keil locate <ws> -p Fan_simulator_Boot.uvprojx -t Fan_simulator_Boot Optim` → 第 317 行，实测该行正是 `Fan_simulator_Boot.uvprojx` 的 `<Optim>7</Optim>`。

### D7 [中] JSON 输出形状随匹配数量/参数而变化 —— 待决策，未修改

- **问题表象**：
  - `emb --json ioc get <ioc> Mcu.Name` → `"STM32H7R7L8Hx"`（标量）
  - `emb --json ioc get <ioc> GPIO`（前缀、仅 1 条）→ `[["GPIO.groupedBy","Show All"]]`（数组）
  - `emb --json keil defines <p> -t <t>` → `["USE_HAL_DRIVER","STM32H7R7xx"]`（字符串数组）
  - `emb --json keil groups <p> -t <t>` → `{"headers":[...],"rows":[[...]]}`（对象）
  - `emb --json keil info <p>` → `{"headers","rows"}`；`emb --json keil info <p> -t <t>` → `[[k,v],...]`（数组）

  同类命令的 JSON 类型无法通过命令名推断，消费方必须对每个"命令 + 参数组合"分支处理。
- **根因**：`src/output/mod.rs` 的 `OutputValue` 用 `#[serde(untagged)]`，四个变体各自序列化成最自然的 JSON；`ioc get` 又在 `src/ioc/mod.rs` 里按"精确命中 → `Message`，前缀命中 → `KeyValue`"选择变体，于是**同一条命令的形状取决于传入的键是否恰好是精确键**——调用方无法预知。
- **解决方案（待选定）**：这是刻意设计，改动等于改输出契约，不单方面拍板。两个方案：
  - **方案 A（小改）**：只把 `ioc get` 统一成一种形状，消除唯一真正不可预测的一处。代价是失去 `emb --json ioc get x.ioc Mcu.Name` 直接拿标量的便利。
  - **方案 B（大改，推荐）**：所有 `--json` 输出加统一的 `kind` 判别字段（`message`/`kv`/`table`/`list`）。Table 形状**增量兼容**（只多一个键，`docs/TEST_SPEC.md` 只断言 `headers`/`rows` 存在）；KeyValue / List / Message 三种会被包裹，属破坏性变更。
  - 倾向：**方案 B**，但需要确认可以接受 KeyValue/List 的破坏性变更（或接受一个过渡期）。
- **验证**：`compare_ioc.py` 的 `ioc get GPIO single-match shape` 一条长期失败，即为本项的常驻回归探针；选定方案后该检查应转为通过。

### D8 [中] 过滤条件无匹配时静默成功 —— 已修复

- **问题表象**：
  - `emb keil config <p> -t <t> nosuch` → 输出为空，**exit 0**
  - `emb keil files <p> -t <t> -g NoGroup` → 仅表头，**exit 0**
  - 对比：`emb keil locate <p> -t <t> NoSuchTag` → `Error: config key 'NoSuchTag' not found`，exit 1 ✓（同一工具内两种截然不同的失败语义）

  Agent 无法区分"确实没有匹配项"与"命令执行成功"，容易把空结果当成有效答案。
- **根因**：`keil config <category>` 走的是 `display_config()`——过滤后直接渲染，`pairs` 为空时照样打出一张空表并返回 `Ok(())`；`keil files -g <group>` 在 `handle()` 里用 `if g.name != *filter { continue; }` 静默跳过，没有任何"过滤条件本身不存在"的判断。两者都缺少 `locate` 已有的目标校验步骤。
- **解决方案**：`keil config <category>` 无匹配时返回 `no configuration keys match category 'X' (available: ...)`（可选分类由键列表**推导**，不硬编码，避免新增键后提示过期）；`keil files -g <group>` 无匹配时返回 `group 'X' not found (available: ...)`。两者均 exit 1，与 `locate` 已有行为一致。
- **验证**（2026-09-18）：新增 `tests/cli_error_paths.rs` 4 条 CLI 测试（2 条断言无匹配必须非零退出并列出可选值，2 条断言合法分类/组仍正常返回，防止修过头）。真实工程上 `emb keil config <Appli> -t ... memory` 等原有用法不受影响。

### D9 [高] `serial daemon` 把 enum 判别值当参数传递，非默认串口参数被静默忽略 —— 已修复

> 首次测试时定级为"低"，复查时发现影响远不止 `meta.toml` 的显示。

- **问题表象（启动不报错）**：`emb serial daemon start COM99`（端口不存在）→ exit 0，仅打印 daemon id；`daemon list` 才显示 `Status: stopped`。`%TEMP%/emb/daemons/<id>/` 目录残留，`meta.toml` 仍写着 `status = "running"`。
- **问题表象（错误处理不一致）**：`daemon stop` / `daemon send` 对不存在的 id 正确报错 exit 1；而 `daemon read` → `(no new data)` exit 0、`daemon history` → `(no history)` exit 0。
- **根因（核心）**：`daemon start` 以隐藏参数重新 spawn 自身，把串口参数写在命令行上。父进程用 `config.data_bits as u8` 取的是 **Rust enum 判别值**（实测 `Five=0`、`Six=1`、`Seven=2`、`Eight=3`；`One=0`、`Two=1`），而子进程按**实际位数**解析：

  ```rust
  let db: u8 = ...parse().unwrap_or(8);
  let data_bits = match db { 5 => Five, 6 => Six, 7 => Seven, _ => Eight };   // 3 → Eight
  let stop_bits = match sb { 2 => Two, _ => One };                          // 1 → One
  ```

  于是 `emb serial daemon start COM3 -d 7 -s 2` 实际以 **8 数据位 / 1 停止位** 运行——静默的数据错帧。只有默认的 `-d 8` 因为落进 `_` 兜底而"碰巧正确"。同样，`meta.toml` 里 `-d 8 -s 1` 被写成 `data_bits = 3`、`stop_bits = 0`。
- **解决方案**：在 `src/serial/port.rs` 增加一对互逆映射 `data_bits_value`/`data_bits_from_value`、`stop_bits_value`/`stop_bits_from_value`（值域 `5..=8` / `1..=2`），父进程写值、子进程读值，两边共用同一份映射；子进程遇到范围外的值直接 `expect` 失败（内部契约被破坏时宁可立刻死掉，也不要静默退回 8N1）。`read`/`history`/`history_clear` 增加 `read_meta(id)?`，与 `stop`/`send` 的错误行为一致。
- **验证**（2026-09-18）：
  - 单测：4 组 bit 设置全部往返一致；判别值 `0..=3` 被拒绝。RED 方式为把 `*_value` 临时还原成 `bits as u8`，观察到 `0 is not a value the parent ever sends`。
  - 实测：`emb serial daemon start COM99 -b 9600 -d 7 -p even -s 2` → `meta.toml` 记录 `data_bits = 7`、`stop_bits = 2`、`parity = "even"`（修复前为 `2` / `1`）。
  - 实测：`daemon read` / `history` / `stop` 对不存在的 id 一律 exit 1 且消息一致；正常 `stop` 后目录被清理。
- **未修**：`daemon start` 对不存在的端口仍返回 exit 0（后台 spawn + 异步失败）。子进程的 stderr 被重定向到 null，失败原因不可见，属于诊断能力问题，未在本轮处理。

### D10 [低] `serial scan` 名称列取值优先级颠倒 —— 已修复

- **问题表象**：本机实测 COM6 显示 `Prolific`、COM4 显示 `Microsoft`；而 WMI 的真实友好名是 `Prolific PL2303GT USB Serial COM Port (COM6)`、`USB Serial Device (COM4)`。
- **根因**：`src/serial/port.rs:47-58` 只在 `manufacturer` 为 `None` 时才回退到 WMI 友好名。USB 串口设备一定带 manufacturer，友好名永不生效；反而蓝牙/AMT 这类非 USB 口显示完整友好名。**而 USB 串口恰是嵌入式开发的主力（ST-Link VCP / CP210x / CH340 / PL2303）**，同一屏下多块板子会全部显示成同一个厂商名，无法区分。
- **解决方案**：`port_label()` 让友好名优先、manufacturer 只作回退；`PortInfo::manufacturer` 更名为 `description`（它的含义已经从"厂商"变成"最佳可读标签"）。
- **验证**（2026-09-18）：本机 6 个串口全部与 WMI 对照一致 ——
  | 端口 | 修复前 | 修复后 = WMI 实际值 |
  |---|---|---|
  | COM4 | `Microsoft` | `USB Serial Device (COM4)` |
  | COM6 | `Prolific` | `Prolific PL2303GT USB Serial COM Port (COM6)` |
  | COM15 | `Prolific` | `Prolific PL2303GT USB Serial COM Port (COM15)` |
  | COM3 / COM8 / COM14 | 本来就对 | 不变 |

  新增 2 条单测钉住优先级（用本机真实名称作样例）。RED 为首次实测记录的错误输出。

### D11 [低] `wLevel` 整张映射表对 AC6 都是错的 —— 已修复

- **问题表象**：`Fan_simulator_Appli_SlotA` 的 `wLevel = 4` → `ccompiler.wlevel:4 (?) [0=None 1=Low 2=Medium 3=High]`。`?` 破坏了 `--ai` 模式"每个值后都带自洽的 `[可选值范围]`"的闭环约定。
- **根因（比初判更严重）**：不是"缺了 4"，而是**整张表都错**。uVision 在 `.uvprojx` 里存的是 **C/C++ 选项卡下拉列表的 0-based 索引**，而原实现的 `0=None 1=Low 2=Medium 3=High` 并非该列表。
- **定表依据**（用户提供 AC6 Warnings 下拉截图为 `<unspecified>` / `No Warnings` / `All Warnings` / `AC5-like Warnings` / `MISRA Compatible`，共 5 项）：
  1. **同文件内的同类先例**：`Optimization` 下拉为 `<default>` / `-O0`…`-Omax` 共 9 项，0-based 索引 —— 这一点由现有 optim 映射与用户截图双重确认。同一 `<Cads>` 块中，`<Optim>0</Optim>` 即 `<default>`。
  2. **"不覆盖"哨兵是 2 而非 0**：`<GroupArmAds><Cads>` 中各未覆盖字段写 `2`（`<interw>2</interw>`、`<uC99>2</uC99>`、`<v6Lto>2</v6Lto>`…），而 `<Optim>0</Optim>`、`<wLevel>4</wLevel>` 是显式取值。说明 0 是合法值而非"未指定"。
  3. **数量吻合**：全工程 `wLevel` 取值为 0（178 处，继承块）、2（Boot）、3（SlotB）、4（SlotA），全部落在 0–4 内。
  4. 用户截图显示当前值为 `AC5-like Warnings`，对应 SlotB 的 3，自洽。

  最终映射：`0=<unspecified>` `1=No Warnings` `2=All Warnings` `3=AC5-like Warnings` `4=MISRA Compatible`。
- **解决方案**：`wlevel_str()` 改为接收 `ac6` 参数（与既有 `optim_str()` 同形），AC6 分支使用上述已核实列表。**AC5 分支的标签（None/Low/Medium/High）未获任何证据，故原样保留并在代码注释中标注"未核实"**，不臆改。
- **顺带对齐**：按用户要求把 AC6 的 Optimization 标签改成 GUI 原文 —— `0 (<default>)`、`6 (-Os balanced)`、`7 (-Oz image size)`，取值范围串同步为 `[0=<default> 1=-O0 … 8=-Omax]`。原为 `default`/`Os`/`Oz`。
- **验证**（2026-09-18）：真实工程三处全部正确 ——
  | Target | wLevel | 输出 |
  |---|---|---|
  | Fan_simulator_Appli_SlotA | 4 | `4 (MISRA Compatible)` |
  | Fan_simulator_Appli_SlotB | 3 | `3 (AC5-like Warnings)` |
  | Fan_simulator_Boot | 2 | `2 (All Warnings)` |

  优化级别同步核对：SlotA/SlotB `0 (<default>)`、Boot `7 (-Oz image size)`，与各自 `.uvprojx` 的 `<Optim>` 一致。新增 3 条单测（AC6 warnings 全表、AC6 optim 全表、越界值仍输出原始数字）。`compare_keil.py` 由 43 项 1 失败变为 **42 项 0 失败**。

### D12 [低] `SchemaVersion` 恒解析为空 —— 已修复

- **问题表象**：`emb --ai keil info <任意真实工程> -t <target>` 解析出的 `schema_version` 恒为空串（该字段当前不展示，故无可见症状，属隐性缺陷）。
- **根因**：真实 Keil5 文件把 schema 版本写成**子元素** `<SchemaVersion>2.1</SchemaVersion>`（`.uvprojx` 与 `.uvmpw` 皆然），而 `src/keil/parser.rs` 读的是 `Project` 节点的同名**属性** `project_node.attribute("SchemaVersion")`，属性不存在 → 恒为空串。
- **为何单测没抓到**：`test_parse_minimal_project` 的样例把 `SchemaVersion` 写成属性形式 `<Project SchemaVersion="1.0">`，与真实文件不符 —— 样例本身是错的，所以测试一直"通过"。
- **解决方案**：改读子元素（`text_of(project_node, "SchemaVersion")`）；把那条样例改成真实格式。
- **验证**：新增单测 `parses_schema_version_from_the_child_element`（用带 `xmlns:xsi` 的完整真实头部），RED 为 `left: "" right: "2.1"`。
- **说明**：该字段目前**没有出现在任何输出里**（`info`/`config` 都不展示），所以修复不改变任何命令的可见行为，只是消除了一个"字段名声称有值、实际恒空"的隐患。

### D13 [低] `file-add` 不校验文件是否存在 —— 待确认，未修改

- **问题表象**：`emb keil file-add <p> -t <t> -g <g> ../../Boot/Core/Src/ghost.c`（磁盘上不存在）→ exit 0 并写入该路径。
- **根因**：`src/keil/editor.rs` 的 `file_add` 只解析并写入 `<File>` 节点，不做任何路径存在性检查，也没有相应提示。
- **解决方案（待选定）**：Keil 自身允许添加尚未创建的文件（先登记后补源码是常见做法），所以"直接拒绝"未必对。倾向**保留 exit 0，但在输出中提示 `warning: file does not exist yet`**——既不挡正常流程，也不让 Agent 以为自己路径写对了。
- **验证**：`test_keil_write.py` 的 `file-add nonexistent path rejected` 一条长期失败，即为本项的常驻回归探针；选定方案后该检查应改为断言"exit 0 且输出含 warning"。

### D14 [提示] `keil map` 的展示细节 —— 部分修复

- **问题表象**：
  - Region 表 `Size` 列给十六进制 + 十进制（`0x600 (1536)`），而 `Max` 列只给十六进制，同一张表两种风格。
  - `Attr` 列只保留最后一个属性：`RW_IRAM1_DTCM_HEAP` 原文为 `ABSOLUTE, UNINIT, COMPRESSED[0x000001e4]`，输出只剩 `COMPRESSED[0x000001e4]`。
  - 只展示 `IROM`/`IRAM`/`XRAM` 三个内存区，H7RS 这类多存储域芯片的真正内存布局（`<OCR_RVCT1..10>`）完全看不到。
- **根因**：
  - `Size` 用 `format!("0x{:X} ({})", ...)`、`Max` 用 `format!("0x{:X}", ...)`，两处写法不一致（`src/keil/mod.rs` 的 `cmd_map`）。
  - 属性提取用 `inside.rsplit(',').next()`，取的是逗号分隔的**最后一节**，而非 `Max:` 字段之后的整段（`src/keil/map.rs` 的 `parse_exec_region`）。
  - `config memory.*` 只解析 Keil 的旧式 `<IROM>/<IRAM>/<XRAM>` 标签，不解析 `<OCR_RVCT*>`。**工具报告的没有错**——`<XRAM>` 在文件里确实是 `0x0/0x0`——只是不完整。
- **解决方案**：
  - 已修复：`Max` 列统一为 `0x{hex} ({dec})`，与 `Size` 列一致。
  - 已修复：属性改为取 `Max:` 之后的完整字符串。
  - **未修改（属功能缺口，非缺陷）**：补齐 `OCR_RVCT*` 需要往 `config` 输出里加一族新的只读键，会与「config 输出的 key 可直接喂给 config-set」的闭环约定冲突，属于需要拍板的新功能，本轮未动。
- **验证**（2026-09-18）：真实 map 实测 —— `RW_IRAM1_DTCM_HEAP` 现在输出 `ABSOLUTE, UNINIT, COMPRESSED[0x000001e4]`，`Max` 列输出 `0x10000 (65536)`。新增单测 `keeps_every_attribute_of_an_execution_region`（用真实多属性行），RED 为 `left: "COMPRESSED[0x000001e4]"`。

### D15 [提示] `tests/run_tests.sh` 无法在干净 clone 上运行，且首条失败即中止 —— 已修复

- **问题表象**：在干净 clone 上执行 `bash tests/run_tests.sh`，26 条用例里 15 条 FAIL，脚本跑到中途直接终止、**不打印 PASS/FAIL 汇总**。原因是 `.gitignore` 忽略 `Test_Project/*`（仅保留 `Test.ioc`），而脚本强依赖 `Test_Project/MDK-ARM/{Project.uvmpw, Boot/Test_Boot.uvprojx, Appli/Test_Appli.uvprojx}`，仓库里并没有这些文件。
- **根因**：脚本开头是 `set -euo pipefail`，而 `run_test` 之外的临时 `out=$(...)` 捕获有 **16 处**没有 `|| true`（`run_test` 里的那处用了 `|| exit_code=$?`，是对的）。emb 返回非零时命令替换的退出码触发 `set -e`，脚本在该点直接退出，后续用例与末尾的 PASS/FAIL 汇总都不执行 —— **任何一条用例失败都可能让整个测试套件静默截断**。
- **解决方案**：
  1. 16 处捕获全部补 `|| true`。**没有**直接去掉 `set -e`：脚本用 `cp`/`mv` 做测试文件的备份还原，`-e` 能在备份失败时立刻停手，避免改坏没有备份的原始文件。
  2. 新增 `check_test_data()`，在 `check_binary` 之后立刻检查 4 个测试文件是否齐备，缺哪个列哪个，并指向 `docs/TEST_SPEC.md`。
  3. `docs/TEST_SPEC.md` 补上 "准备测试数据" 一节（此前只在文档里描述了文件清单，没说这些文件不入库、从哪来）。
- **验证**（2026-09-18）：
  - 用等价的失败命令替换做定点验证：`out=$( (exit 1) | tr -d '\r' )` 在 `set -euo pipefail` 下确实中止（exit 1），加 `|| true` 后正常继续（exit 0）。
  - 实际运行 `bash tests/run_tests.sh`：现在在开头立即报出缺失的 3 个文件并提示准备方法（exit 1），不再跑到中途静默死掉。`Test.ioc` 已在库中，正确地未被列为缺失。

## 6. 未覆盖范围

| 项 | 原因 |
|---|---|
| `keil flash` | 需真实硬件，按要求跳过 |
| `debug keil` 批处理调试 | 需连接调试器（CMSIS-DAP）并会复位目标，按要求跳过。已验证参数校验路径：未配置 UV4 时正确报错 exit 1 |
| `debug openocd` | 实现为存根，固定输出 `OpenOCD support not yet implemented` 并 exit 0 |
| 真实串口收发 / daemon 长期稳定性 | 需真实串口设备 |
| 多 Target 大批量构建压测 | 仅对 Boot（47 文件）做了完整增量/全量构建；Appli（296 文件）未做全量 rebuild 以控制耗时 |

## 7. 修复优先级建议

| 优先级 | 项 | 理由 |
|---|---|---|
| 优先级 | 项 | 状态 | 说明 |
|---|---|---|---|
| P0 | D1 构建失败返回 0 | **已修复** | 直接导致自动化流程误判成功 |
| P0 | D2 `keil map` 列错位 | **已修复** | 输出错误数据且与自身 `build` 命令矛盾 |
| P1 | D4 路径自动探测 | **已修复** | 首次使用即受阻 |
| P1 | D5 `ioc generate` 输入校验 / 路径 / 超时提示 | **部分修复** | 弹窗等待属 CubeMX 限制；完成判定路径待复测 |
| P1 | D11 `wLevel` 映射表 | **已修复** | 整表对 AC6 失效 |
| P1 | D3 XML 格式破坏 | 待修 | 方案需先定（本次未动） |
| P2 | D6 workspace `-p` | **已修复** | 5 个子命令补齐 |
| P2 | D7 JSON 输出形状 | 待决策 | 改输出契约，需你选方案 A/B |
| P2 | D8 无匹配静默成功 | **已修复** | |
| P2 | D9 daemon 串口参数 | **已修复**（定级由"低"升为"高"） | 非默认数据位被静默忽略 |
| P3 | D10 `serial scan` 名称 | **已修复** | |
| P3 | D12 `SchemaVersion` | **已修复** | 不改变可见行为，消除隐患 |
| P3 | D13 `file-add` 校验 | 待确认 | 建议保留 exit 0 + 输出提示 |
| P3 | D14 `keil map` 展示 | **部分修复** | `OCR_RVCT*` 可见性属新功能 |
| P3 | D15 `run_tests.sh` | **已修复** | |

## 8. 修复记录（2026-09-18）

累计改动：11 个既有文件 `+868 −123`，另加 5 个新测试文件与本文档。

### 8.1 P0

| 文件 | 改动 |
|---|---|
| `src/keil/mod.rs` | `cmd_build` 增加 target 校验；末尾 `Ok(())` → `ensure_build_succeeded(&result)`；新增该函数 |
| `src/keil/map.rs` | Grand Totals 列映射修正、删除无来源的 `grand` 字段、重写注释；修正 1 条固化错误映射的旧断言 |
| `tests/cli_build_exit_code.rs` | 新增，4 条 CLI 级回归 |

### 8.2 P1

| 文件 | 改动 |
|---|---|
| `src/keil/mod.rs` | `wlevel_str()` 改为 `wlevel_str(level, ac6)`，AC6 分支使用已核实的 5 档列表；`optim_str()` AC6 标签对齐 GUI 原文 |
| `src/keil/builder.rs` | `find_uv4()` 新增注册表查询与 `%LOCALAPPDATA%` 候选；新增 `uv4_from_registry()` / `parse_reg_path()` / `uv4_beside_arm_dir()` 与 3 条单测 |
| `src/ioc/generator.rs` | `find_cubemx()` 新增 `%LOCALAPPDATA%` 候选；`generate()` 新增输入文件校验、`project_dir_of()` 修 `current_dir("")`、临时脚本名加 uuid、超时提示报出 PID 与停止命令；新增 `find_cubemx_pids()` 与 4 条单测 |
| `tests/cli_ioc_generate.rs` | 新增，1 条 CLI 级回归（断言快速失败、不启动 CubeMX） |

### 8.3 P2 / P3

| 文件 | 改动 |
|---|---|
| `src/keil/mod.rs` | `config`/`files` 过滤无匹配时报错（新增 `config_categories()` 推导可选值）；`cmd_build` 改用共用的 `resolve_path` 并接收 `-p`；`locate` 接收 `-p`；`cmd_map` 的 `Max` 列格式对齐 `Size` 列 |
| `src/main.rs` | `Build`/`Rebuild`/`Clean`/`Flash`/`Locate` 补 `-p/--project`；daemon 子进程改用共用映射解析串口参数，越界值立即失败 |
| `src/keil/map.rs` | 执行区域属性改为完整提取（原来只留最后一节） |
| `src/keil/parser.rs` | `SchemaVersion` 改读子元素；修正那条用属性形式的错误样例 |
| `src/serial/port.rs` | 新增 `data_bits_value`/`from_value`、`stop_bits_value`/`from_value` 互逆映射；`port_label()` 让 WMI 友好名优先；`PortInfo::manufacturer` → `description` |
| `src/serial/daemon.rs` | 用共用映射写命令行与 `meta.toml`；`read`/`history`/`history_clear` 增加存在性检查 |
| `src/serial/mod.rs` | 跟随字段更名 |
| `tests/run_tests.sh` | 16 处命令替换补 `|| true`；新增 `check_test_data()` 开局校验 |
| `docs/TEST_SPEC.md` | 新增"准备测试数据"一节，说明 `Test_Project/*` 不入库及其准备要求 |

### 8.4 回归结果（截至本轮）

| 检查 | 结果 |
|---|---|
| `cargo test`（单测） | **80 passed / 0 failed**（原 59） |
| `cargo test`（集成，4 个测试二进制） | **14 passed / 0 failed** |
| `cargo clippy --all-targets` | 7 条警告，全部为改动前既有，**无新增**（`editor.rs:240`、`parser.rs:394`、`daemon.rs:275/311`、`protocol.rs:7`、`keil_debug.rs:38`、`output/mod.rs:50`） |
| `compare_keil.py` 对拍 | **42 项 0 失败** |
| `compare_ioc.py` 对拍 | 17 项 1 失败（仅剩 D7，待决策） |
| `test_keil_write.py` | 36 项 6 失败（D3 格式 5 项 + D13，均未修） |
| `verify_map_fix.py` | 三份真实 map 的 armlink 恒等式全部通过 |
| 真实工程只读复测 | wLevel/optim、workspace `-p` 路由、`locate` 行号、`keil map`、`serial scan` 全部与独立基准一致 |

### 8.5 TDD 执行说明

| 测试 | RED 方式 |
|---|---|
| D1 `failed_build_is_an_error` | 实现前先把函数架空成 `Ok(())`，观察断言失败 |
| D1 `tests/cli_build_exit_code.rs` 4 条 | 实现前运行，3 条失败于 `failed to locate UV4.exe`（正是要修的行为） |
| D2 `grand_totals_columns_satisfy_armlink_identities` | 实现前运行，失败于 `left: 4176, right: 5408` |
| D2 `total_rom_is_read_from_the_map_not_computed` | 实现后补写，通过临时把映射还原为旧写法确认失败（`left: 6068, right: 74804`）后才恢复 |
| P1 各单测 | 实现前运行，失败于函数缺失 |
| D5 `tests/cli_ioc_generate.rs` | **未单独跑 RED**，理由见 D5 条目 |
| D6 `tests/cli_workspace_project.rs` 4 条 | 实现前运行，全部失败于 `unexpected argument '-p' found` |
| D8 `tests/cli_error_paths.rs` 4 条 | 实现前运行，2 条失败于"无匹配却 exit 0"，2 条通过（防止修过头） |
| D9 串口参数往返 2 条 | 把 `*_value` 临时还原成 `bits as u8`，失败于 `0 is not a value the parent ever sends` |
| D9 `daemon_read_and_history_reject_an_unknown_id` | 实现前运行，失败于 exit 0 |
| D10 `port_label` 2 条 | 用首次测试记录的错误输出作为 RED 依据（`COM6 → Prolific`） |
| D12 `parses_schema_version_from_the_child_element` | 实现前运行，失败于 `left: "" right: "2.1"` |
| D14 `keeps_every_attribute_of_an_execution_region` | 实现前运行，失败于 `left: "COMPRESSED[0x000001e4]"` |
| D15 `|| true` 补漏 | 用等价失败命令替换定点验证：旧写法 exit 1 中止、新写法 exit 0 继续 |

### 8.6 原工程影响

`Fan_simulator_mdk` 全程只读。所有写类验证（`config-set` / `defines-*` / `includes-*` / `group-*` / `file-*` / `ioc set|rm`）均在 `%TEMP%` 下的副本执行，用后删除。

当天该工程出现过两批非本工具写入的文件：`.codegraph/*`（用户自己的 CodeGraph 守护进程）、以及 14:11:12 的 `*.uvguix.*` / `*.uvgui.*` / `Project.uvmpw`（Keil uVision 打开工作区时写入，当时 `UV4.exe` 正在运行；其中 GUI 文件被 `.gitignore` 忽略）。用户于 14:12:53 提交了自己的未提交改动（`8ff9e54`）。
