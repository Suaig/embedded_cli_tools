# emb 工具对 Fan_simulator_mdk 工程的实测报告
> 用真实 STM32H7RS 双工程 Keil workspace 验证 `emb` 的解析正确性、功能完整性与健壮性，并给出缺陷清单。
**更新**: 2026-09-18 · **关联**: `src/keil/*`、`src/ioc/*`、`src/serial/*`、`tests/*.rs`、`docs/plan/测试验收_规格.md` · **状态**: 定稿

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

### D3 [高] Keil 写操作全量重写 XML，破坏文件格式 —— 已修复

- **问题表象**：对同一宏 `defines-add` 再 `defines-remove`（净零改动）后，**1836 行中 1829 行发生变化**，文件 72932 → 70533 字节：

  | 破坏点 | 原始 | 重写后 |
  |---|---|---|
  | 命名空间属性 | `xsi:noNamespaceSchemaLocation="..."` | `noNamespaceSchemaLocation="..."`（**前缀丢失**） |
  | 空元素 | `<MiscControls></MiscControls>` | `<MiscControls />` |
  | 空行 | 保留 | 全部删除 |
  | 属性顺序 | `Cclass= Cgroup= Cvendor= ...` | 重排（DOM 用哈希存属性） |
  | 文件结尾 | 有换行 | 无换行 |
  | XML 声明 | `standalone="no"` | 丢失 |

- **影响**：任何 `config-set` / `*-add` / `*-remove` 都让 git diff 变成整文件，code review 无法定位真实改动；`xsi:` 前缀丢失使 schema 位置对标准 XML 解析器失效。
- **根因**：`src/keil/editor.rs` 的 `save_xml` 用 xmltree 的 DOM 全量解析再序列化；DOM 只保留树结构和文本，不保留空白布局、属性顺序与命名空间前缀的书写形式。
- **解决方案（采用"序列化后格式化补丁"）**：`save_xml` 在写出后，用**原文**把 DOM 丢掉的格式补回来 —— 还原 XML 声明、直接沿用原文的根元素开标签（连 `xsi:` 前缀一起）、按原文件习惯逐个还原空元素风格（Keil 同一文件里 `<Tag></Tag>` 与 `<Tag/>` 混用，所以只对"原文始终自闭合"的标签保持自闭合，且连 `/>` 前那个空格一起还原）、补结尾换行。四条规则**都不需要位置信息**。
  - 选择这个方案而不是"按 `locate` 行区间做文本替换"：先测了收益 —— 补丁把净零往返从 1508 行降到 10 行（99.3%），而文本替换需要重写全部 9 个编辑命令，收益只多最后 10 行。
- **验证**（2026-09-18）：
  - 真实文件实测：1836 行的工程做净零往返，**变化行数 1508 → 10**。残留 10 行 = 6 行被删的空行 + 2 个元素（`<component>` / `<package>`）的属性顺序，两者都是 DOM 结构上无法恢复的。
  - `save_xml` 原有的"写回后重新解析校验"步骤在补丁之后执行，因此补丁本身也被纳入校验 —— 补丁若破坏 XML 会立即失败。
  - 新增 4 条单测（声明/根标签/结尾换行、成对风格、保持自闭合、混用标签不误判）。
  - 对拍脚本 `test_keil_write.py` 从"36 项 6 失败"变为"37 项 1 失败"，唯一剩下的就是属性顺序那一项。

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
  4. **把"等待应答"当成"失败"**：固定 120 s 到点就报 `generation timed out`，而 CubeMX 此时很可能只是开着对话框等人点。现在分两种情况报：
     - 进程已退出 → "`.uvprojx` 未产出且 CubeMX 已退出，去看日志"（这是真失败）；
     - 进程仍在 → "**CubeMX 仍在运行（PID …）**，十有八九在等对话框；`-q` 不会自动应答；请应答后重跑、或加大 `--timeout`、或 `taskkill` 停掉它"。

     并新增 `--timeout <seconds>`（默认 120），知道要手动点对话框的人可以放宽。措辞上不再说"失败"。
- **未修复 / 设计取舍**：
  - 超时后**不自动 kill** CubeMX 进程。自动终止看似更干净，但超时窗口恰好覆盖"用户正要点确认"的场景——杀掉会连带丢弃用户即将完成的生成。改为报告 PID + 停止命令，把决定权交回用户。
  - 完成判定写死为 `<ioc 所在目录>/MDK-ARM/<ioc 文件名>.uvprojx`。本工程 ioc 在工程根、Keil 工程在 `MDK-ARM/Appli|Boot/` 子目录，若 CubeMX 确实按 `ProjectStructure` 写回子目录，则该判定路径永不成立。**仍需在"有人点确认"的前提下复测才能定论**（本次日志停在 `IP not ready for code generation: LWIP`，看不出目标路径）。
  - 脚本把 `project name` 固定为 ioc 文件名（`Fan_simulator`），而工程实际子项目名是 `Fan_simulator_Appli` / `Fan_simulator_Boot`，是否会另生成孤立工程同样待复测。
  - 自动点掉对话框需要截图 + 鼠标操作（computer use 一类能力），CLI 本身做不到；工具的职责是把"正在等一个人"讲清楚，让上层 Agent 决定是否去驱动 GUI。
- **验证**（2026-09-18）：`emb ioc generate nosuch.ioc`（无任何配置）→ `Error: ioc file not found: nosuch.ioc`，exit 1，166 ms 返回；新增 `tests/cli_ioc_generate.rs` 1 条 CLI 测试（断言快速失败、不启动 CubeMX）。`current_dir`、PID 定位、超时消息分支各新增单测（消息含 PID/`--timeout`/`taskkill`，进程已退出时指向日志）。
- **测试披露**：`tests/cli_ioc_generate.rs` 未单独跑一次 RED。同一场景（`emb ioc generate nosuch.ioc`）在修复前已实测观察到"启动 CubeMX → 120 s 超时 → 提示未提及文件不存在"的完整行为，即该测试要断言的失败现象；重跑只会再造一个 1.6 GB JVM 而无新增信息。

### D6 [中] workspace 下 `build`/`rebuild`/`clean`/`flash`/`locate` 不支持 `-p` —— 已修复

- **问题表象**：`emb keil build <ws> -p <proj>` → `error: unexpected argument '-p' found`（exit 2）。`emb keil build <ws> -t "Fan_simulator_Boot"` 会静默产出 `Success: no`——因为 `cmd_build` 忽略子项目筛选、固定取 workspace 第一个子项目（Appli），而 Appli 里没有该 target。
- **影响**：无法从 workspace 构建 Appli 以外的子工程；与 README「所有 keil 子命令支持 workspace 多项目」的表述不符（`info`/`config`/`defines` 等读命令确实支持）。
- **根因**：`cmd_build` 签名不含 `project` 参数，内部 `resolve_project_path(path_str, &None)` 写死为 `None`；`main.rs` 的 `Build`/`Rebuild`/`Clean`/`Flash`/`Locate` 变体未定义 `-p`。
- **解决方案**：5 个子命令均补 `-p/--project`；`cmd_build` 改为走与其他子命令相同的 `resolve_path(path_str, project)`（顺带删掉了 `cmd_build` 里那段与 `resolve_path` 逐字重复的路径解析）；`locate` 改为先解析再读取。
- **验证**（2026-09-18）：
  - 新增 `tests/cli_workspace_project.rs` 4 条 CLI 测试。用两个子项目（`TargetA`/`TargetB`、`DEFINE_A`/`DEFINE_B`）证明 `-p` 真的改变了路由：不带 `-p` 时错误消息列出 `TargetA`，带 `-p b.uvprojx` 时列出 `TargetB`。
  - 真实工程：`emb keil build <Project.uvmpw> -p Fan_simulator_Boot.uvprojx -t NoSuchTarget` → 列出 `Fan_simulator_Boot`（此前是 `Success: no` 静默失败）；`emb keil locate <ws> -p Fan_simulator_Boot.uvprojx -t Fan_simulator_Boot Optim` → 第 317 行，实测该行正是 `Fan_simulator_Boot.uvprojx` 的 `<Optim>7</Optim>`。

### D7 [中] JSON 输出形状随匹配数量/参数而变化 —— 已修复（方案 B）

- **问题表象**：
  - `emb --json ioc get <ioc> Mcu.Name` → `"STM32H7R7L8Hx"`（标量）
  - `emb --json ioc get <ioc> GPIO`（前缀、仅 1 条）→ `[["GPIO.groupedBy","Show All"]]`（数组）
  - `emb --json keil defines <p> -t <t>` → `["USE_HAL_DRIVER","STM32H7R7xx"]`（字符串数组）
  - `emb --json keil groups <p> -t <t>` → `{"headers":[...],"rows":[[...]]}`（对象）
  - `emb --json keil info <p>` → `{"headers","rows"}`；`emb --json keil info <p> -t <t>` → `[[k,v],...]`（数组）

  同类命令的 JSON 类型无法通过命令名推断，消费方必须对每个"命令 + 参数组合"分支处理。
- **根因**：`src/output/mod.rs` 的 `OutputValue` 用 `#[serde(untagged)]`，四个变体各自序列化成最自然的 JSON；`ioc get` 又在 `src/ioc/mod.rs` 里按"精确命中 → `Message`，前缀命中 → `KeyValue`"选择变体，于是**同一条命令的形状取决于传入的键是否恰好是精确键**——调用方无法预知。
- **解决方案（方案 B，已实现）**：`OutputValue` 改为**手写 `Serialize`**，每个变体都带 `kind` 判别字段：
  | kind | 字段 |
  |---|---|
  | `message` | `value` |
  | `kv` | `entries` |
  | `table` | `headers`、`rows`（**保持顶层**，因此这一种是增量兼容） |
  | `list` | `items` |
  | `sections` | `sections`（见 D16） |

  之所以手写而不是用 `#[serde(tag = ...)]`：internally-tagged 不支持 newtype 变体（`Message(String)`），adjacently-tagged（`tag` + `content`）又会把 `headers`/`rows` 推进 `content` 里，破坏 README 与 `docs/plan/测试验收_规格.md` 已记录的表格形状。手写既保住了顶层字段，也不用改任何一处调用点。
- **验证**（2026-09-18）：新增 `tests/cli_json_shape.rs` 4 条 CLI 测试；`src/output/mod.rs` 新增 3 条单测（四种 kind、表格顶层字段、`ioc get` 两种形状可区分）。实测：

  ```
  $ emb --json ioc get project.ioc Mcu.Name
  { "kind": "message", "value": "STM32H7R7L8Hx" }
  $ emb --json ioc get project.ioc GPIO
  { "kind": "kv", "entries": [["GPIO.groupedBy", "Show All"]] }
  ```

  `compare_ioc.py` 的常驻探针由"长期 1 失败"变为 **25 项 0 失败**。`docs/plan/测试验收_规格.md` 的 IT-FMT-01/02 断言的是"合法 JSON""`headers`/`rows` 存在"，仍然成立（表格形状未变）。

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
- **AC5 分支更正（2026-09-22）**：用户提供 AC5 的 Warnings 下拉截图，只有 **3 项** —— `<unspecified>` / `No Warnings` / `All Warnings`，与 AC6 前 3 项同名同序。可确认是 AC5 页而非 AC6 页的依据：该页下方带 `C99 Mode` 复选框，AC6 页无此项；且列表在 `All Warnings` 后即结束，没有 AC6 的第 4、5 项。原 `None/Low/Medium/High` 纯属臆测，已改为 `0=<unspecified> 1=No Warnings 2=All Warnings`，并补 1 条单测。副作用：AC5 工程若出现 `wLevel ≥ 3`，现在显示 `3 (?)` —— 这正是"读到越界值就照实报"的预期行为，而非静默错标。
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

### D13 [低] `file-add` 不校验文件是否存在 —— 已修复

- **问题表象**：`emb keil file-add <p> -t <t> -g <g> ../../Boot/Core/Src/ghost.c`（磁盘上不存在）→ exit 0 并写入该路径，没有任何提示。
- **根因**：`src/keil/editor.rs` 的 `file_add` 只解析并写入 `<File>` 节点，不做路径存在性检查，也没有相应输出。
- **解决方案**：Keil 自身允许添加尚未创建的文件（先登记后补源码是常见做法），所以**保留 exit 0**，但在 `<FilePath>` 相对 `.uvprojx` 目录解析不到文件时向 stderr 打 `warning: '<path>' does not exist yet (looked for <resolved>)`。选 stderr 而非 stdout，是为了不动 `--json` / `--ai` 已有的输出契约。
- **验证**（2026-09-18）：新增 2 条 CLI 测试 —— 不存在的路径必须 exit 0 且 stderr 含该路径名；存在的路径必须不产生任何 warning（防止修过头）。`test_keil_write.py` 的对应检查由"必须报错"改为"exit 0 且 stderr 含路径名"，两条均通过。

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
  2. 新增 `check_test_data()`，在 `check_binary` 之后立刻检查 4 个测试文件是否齐备，缺哪个列哪个，并指向 `docs/plan/测试验收_规格.md`。
  3. `docs/plan/测试验收_规格.md` 补上 "准备测试数据" 一节（此前只在文档里描述了文件清单，没说这些文件不入库、从哪来）。
- **验证**（2026-09-18）：
  - 用等价的失败命令替换做定点验证：`out=$( (exit 1) | tr -d '\r' )` 在 `set -euo pipefail` 下确实中止（exit 1），加 `|| true` 后正常继续（exit 0）。
  - 实际运行 `bash tests/run_tests.sh`：现在在开头立即报出缺失的 3 个文件并提示准备方法（exit 1），不再跑到中途静默死掉。`Test.ioc` 已在库中，正确地未被列为缺失。

### D16 [中] `--json` 有三条命令会连续打印两个 JSON 文档 —— 已修复

> 实现 D7 方案 B 时发现的同族问题。

- **问题表象**：`emb --json keil map <map>` 的 stdout 是**两个 JSON 对象接在一起**，整体不是合法 JSON：

  ```
  $ emb --json keil map out.map | jq .
  parse error: Extra data: line 38 column 1 (char 482)
  ```

  `emb --json keil locate <proj> -t <T> <el>` 同样（`kv` 段 + 原文片段段）。**同族的第三处是 `emb --json debug keil <proj> -t <T>`**，修 `keil map`/`locate` 时漏掉，2026-09-22 补修。三处根因相同：`cmd_map` / `Locate` / `debug::handle` 各调了两次 `output::display()`，而 `display` 每次输出一个完整文档。
- **影响**：`--json` 的全部价值是能被 `jq` / 脚本直接消费，这两条命令直接违背该契约；`--ai` / human 模式不受影响（本来就是两段文本）。
- **解决方案**：新增 `OutputValue::Sections(Vec<OutputValue>)` —— JSON 下合成单个 `{"kind":"sections","sections":[…]}`，human/AI 下逐段渲染并用空行分隔，**外观与原来的两次 `display()` 完全一致**。`keil map` 与 `keil locate` 改为一次 `display`；`debug keil` 于 2026-09-22 同步改为一次 `display`，并顺带接上 `DebugResult.uv4_log`（见 D17）。
- **验证**（2026-09-18）：新增 `tests/cli_json_shape.rs` 4 条 CLI 测试 —— `keil map` / `keil locate` 的 stdout 必须能被 `serde_json::from_str` 一次解析、且段数与 `kind` 正确；`ioc get` 精确/前缀两种命中的 `kind` 必须不同；`keil info` 的表格必须保持 `headers`/`rows` 在顶层。实测 `emb --json keil map out.map | jq` 现在可用；human 模式输出逐字节未变。

### D17 [低] `DebugResult.uv4_log` 被填充但从未被读取 —— 已修复（2026-09-22）

> 清理 clippy 警告时发现。定级"低"是因为不影响任何已有输出；但它的实际价值在超时场景，见下。

- **问题表象**：`cargo clippy` 报 `field 'uv4_log' is never read`（`src/debug/keil_debug.rs`）。
- **根因**：`run()` 把 UV4 `-o <log>` 的内容读进 `DebugResult`，但 `debug::handle` 只用了 `exit_code` / `timed_out` / `dump`，日志被丢弃。
- **为什么不该直接删字段**：UV4 自身的日志记录的是"UV4 进程做了什么"——连不上调试器、target 不存在、license 问题等；而 `dump` 是 `.ini` 调试命令的输出（内存、寄存器），两者互补。`UV4 -d` 超时且 `dump` 为空时，这条日志往往是唯一线索。与 D5 同一条设计原则：**卡住时要说清楚卡在哪，而不是只报一个"超时"**。另外日志写在 `tempfile::tempdir()` 里，`run()` 返回时即被删除，不留在 `DebugResult` 里就彻底丢了。
- **解决方案**：新增 `keil_debug::log_tail()`，返回日志最后 20 行非空内容（`Option<String>`，日志为空时 `None`）。`debug::handle` 仅在 **`timed_out` 或 `exit_code != 0`** 时把它作为第三个 `Sections` 段输出，正常退出时输出与改动前完全一致。
- **验证**（2026-09-22）：新增 3 条单测覆盖 `log_tail`（空日志返回 `None`、短日志整段保留、长日志从头截断且不切断行）。`cargo test` 93 单测全过。

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
| P1 | D11 `wLevel` 映射表 | **已修复** | 整表对 AC6 失效；AC5 分支曾为臆测，已于 2026-09-22 按截图更正 |
| P1 | D3 XML 格式破坏 | **已修复** | 序列化后格式化补丁，净零往返 1508 → 10 行 |
| P2 | D7 JSON 输出形状 | **已修复**（方案 B：`kind` 判别字段） | KeyValue/List/Message 的包裹是已接受的破坏性变更 |
| P2 | D16 `--json` 输出多个文档 | **已修复** | `keil map` / `keil locate` / `debug keil` 的 stdout 整体不是合法 JSON |
| P2 | D6 workspace `-p` | **已修复** | 5 个子命令补齐 |
| P2 | D8 无匹配静默成功 | **已修复** | |
| P2 | D9 daemon 串口参数 | **已修复**（定级由"低"升为"高"） | 非默认数据位被静默忽略 |
| P3 | D10 `serial scan` 名称 | **已修复** | |
| P3 | D12 `SchemaVersion` | **已修复** | 不改变可见行为，消除隐患 |
| P3 | D13 `file-add` 校验 | **已修复** | 保留 exit 0，改打 stderr warning |
| P3 | D14 `keil map` 展示 | **部分修复** | `OCR_RVCT*` 可见性属新功能 |
| P3 | D15 `run_tests.sh` | **已修复** | |
| P3 | D17 `DebugResult.uv4_log` 未读取 | **已修复** | 清 clippy 时发现；接上后 UV4 超时才看得出卡在哪 |

## 8. 修复记录（2026-09-18）

累计改动：11 个既有文件 `+868 −123`，另加 5 个新测试文件与本文档。

### 8.7 第二轮：D3 / D5 / D7 / D13 + D16（2026-09-18 同日）

| 项 | 改动 |
|---|---|
| D3 | `src/keil/editor.rs` 新增 `restore_keil_format()` 及其 4 个辅助函数（声明/根标签/空元素风格/结尾换行），`save_xml` 写出后调用；新增 4 条单测 |
| D5 | `src/ioc/generator.rs` 抽出 `timeout_message()`、`generate()` 增加 `timeout_secs` 参数；`src/ioc/mod.rs` 与 `src/main.rs` 增加 `--timeout`；新增 2 条单测 |
| D7 | `src/output/mod.rs` 去掉 `#[serde(untagged)]`、改为手写 `Serialize` 加 `kind`；新增 3 条单测 |
| D16 | `src/output/mod.rs` 新增 `Sections` 变体；`human.rs`/`ai.rs` 各加一个渲染分支；`keil/mod.rs` 的 `cmd_map` 与 `locate` 改为一次 `display`；新增 `tests/cli_json_shape.rs` 4 条 CLI 测试 |
| D13 | `src/keil/editor.rs` 的 `file_add` 增加路径存在性提示（stderr）；新增 2 条 CLI 测试 |
| — | 文档同步：`README.md`（`-p` 签名、optim/wlevel 示例、JSON 契约表）、`skills/keil-dev/references/{commands,editing}.md`、`docs/plan/测试验收_规格.md`（JSON 用例与验收清单） |

第二轮回归结果：

| 检查 | 结果 |
|---|---|
| `cargo test`（单测） | **89 passed / 0 failed** |
| `cargo test`（集成，5 个测试二进制） | **20 passed / 0 failed** |
| `cargo clippy --all-targets` | 仍是那 7 条既有警告，**无新增** |
| `compare_keil.py` | **42 项 0 失败** |
| `compare_ioc.py` | **25 项 0 失败**（D7 的常驻探针已消除） |
| `test_keil_write.py` | **37 项 1 失败**（仅剩 DOM 不保属性顺序这一已知残留） |
| `verify_map_fix.py` | 三份真实 map 恒等式全过 |
| D3 端到端 | 真实 1836 行工程净零往返：**1508 → 10 行** |

### 8.8 第三轮：AC5 `wLevel` 更正 / D16 第三处 / clippy 归零（2026-09-22）

| 项 | 改动 |
|---|---|
| D11（AC5 分支） | `src/keil/mod.rs` 的 `wlevel_str()` AC5 分支由臆测的 `None/Low/Medium/High` 改为截图证实的 `0=<unspecified> 1=No Warnings 2=All Warnings`；新增 1 条单测 |
| D16（第三处） | `src/debug/mod.rs` 的 `debug keil` 由两次 `display` 改为一次 `Sections` |
| D17 | `src/debug/keil_debug.rs` 新增 `log_tail()` 及 3 条单测；`debug::handle` 仅在超时/非零退出时输出 UV4 日志尾部 |
| 清理 | 删 `output::not_implemented`（死代码）；`parser.rs` 的 `match` 改 `matches!`；`editor.rs` 去掉多余的 `*`；`daemon.rs` 两处 `&[]` 改 `[]`；`protocol.rs` 改用 `is_multiple_of` |
| 文档 | `docs/TEST_SPEC.md` → `docs/plan/测试验收_规格.md`（按 docs 规范归位并补头部）；引用同步至 `README.md`、`tests/run_tests.sh`、本报告 |

第三轮回归结果：

| 检查 | 结果 |
|---|---|
| `cargo test`（单测） | **93 passed / 0 failed**（+4） |
| `cargo test`（集成，5 个测试二进制） | **20 passed / 0 failed** |
| `cargo clippy --all-targets` | **0 警告**（此前 7 条） |

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
| `docs/plan/测试验收_规格.md` | 新增"准备测试数据"一节，说明 `Test_Project/*` 不入库及其准备要求 |

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
