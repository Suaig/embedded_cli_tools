# emb keil / ioc 命令速查

`<proj>` = `.uvprojx` 路径；`<T>` = target 名（多 target 必填，单 target 可省）。全局选项 `--ai`（紧凑）/ `--json`。

`.uvmpw` 工作区：所有**接受工程路径**的 keil 子命令都接受 `-p <project>`（子项目文件名如 `Boot.uvprojx`，或 `.\Boot\Boot.uvprojx` 完整相对路径）；不带 `-p` 时取工作区里的**第一个**子项目。（`emb keil map` 例外——它接收的是 `.map` 文件，与工作区无关。）

## 查看（只读）

| 命令 | 用途 |
|---|---|
| `emb keil info     <proj> [-t <T>]` | 工程概览（targets / 芯片 / 输出目录）；带 `-t` 时另附该 target 的内存区表 |
| `emb keil config   <proj> -t <T> [category]` | target 配置 |
| `emb keil defines  <proj> -t <T>` | 预处理宏 |
| `emb keil includes <proj> -t <T>` | include 路径 |
| `emb keil groups   <proj> -t <T>` | 源文件分组 |
| `emb keil files    <proj> -t <T> [-g GROUP]` | 文件列表（可按 group 过滤） |

上表均可加 `[-p <project>]`。

`config` 的 `category` 只接受顶层分类（`device`/`output`/`ccompiler`/`asm`/`linker`/`memory`），**不接受具体键**（`ccompiler.optim` 会报错并列出可选分类）。`config <category>` 与 `files -g <group>` 在无匹配时返回非零退出码，不会静默给空结果。

`config memory.irom` / `iram` / `xram` 读的是 Keil 的老式标签，其中 `xram` 在多数工程里是空的历史字段——**别拿它判断"没有第二块 RAM"**。真正生效的内存区在 `<OCR_RVCT1..10>` 里，用 `keil info -t <T>` 看（列出所有 size 非零的区，含 ROM/RAM 类型）。

## 定位（省 token，见 editing.md）

```
emb keil locate <proj> -t <T> defines       # 宏节点（start_line + raw）
emb keil locate <proj> -t <T> ScatterFile   # 任意配置标签
emb keil locate <proj> -t <T> File          # 多行元素（start_line + end_line）
```

可加 `[-p <project>]`。`locate` 是**只读**的，返回行号供 Read + Edit 使用，不会重写 XML。

## 修改工程（editor 快速模式，见 editing.md 的格式警告）

```
emb keil defines-add    <proj> -t <T> MACRO
emb keil defines-remove <proj> -t <T> MACRO
emb keil includes-add    <proj> -t <T> path
emb keil includes-remove <proj> -t <T> path
emb keil group-add       <proj> -t <T> NAME
emb keil group-remove    <proj> -t <T> NAME
emb keil group-rename    <proj> -t <T> OLD NEW
emb keil file-add        <proj> -t <T> -g GROUP filepath
emb keil file-remove     <proj> -t <T> -g GROUP filename
emb keil file-exclude    <proj> -t <T> -g GROUP filename   # 不编译
emb keil file-include    <proj> -t <T> -g GROUP filename   # 恢复编译
emb keil config-set      <proj> -t <T> key value
```

## 构建 / 烧录

```
emb keil build   <proj> [-t <T>] [-p <project>]    # 增量
emb keil rebuild <proj> [-t <T>] [-p <project>]    # 全量重编译
emb keil clean   <proj> [-t <T>] [-p <project>]
emb keil flash   <proj> [-t <T>] [-p <project>]    # Flash Download
```
返回 success / errors / warnings / program_size(Code/RO/RW/ZI) / output_file。UV4 码：0=成功，1=警告，2+=错误。

**失败会返回非零退出码**，stderr 带错误/警告计数与 UV4 日志末尾几行。target 名写错会在调用 UV4 **之前**就被拒掉并列出可用 target，不会白跑一轮编译。

`.uvmpw` 下不带 `-p` 只会构建工作区第一个子项目；要构建别的子项目必须显式 `-p`。

## map 分析

```
emb keil map Objects/STM32H723/STM32H723.map
```
输出 ROM/RW/ZI 总量 + 各执行区域。核对内存布局（DTCM/AXI SRAM/D3 SRAM 分配）。

## 调试（UV4 -d 脚本化，见 debug.md）

```
emb debug keil <proj> -t <T> --regs
emb debug keil <proj> -t <T> --read 0x58001408[%16]
emb debug keil <proj> -t <T> --dump 0x20000000,0x20000100
emb debug keil <proj> -t <T> --break main [--run-to main] [--step 5] [--regs]
emb debug keil <proj> -t <T> --ini custom.ini [--timeout 60] [--no-reset]
```

## ioc（STM32CubeMX，辅助）

```
emb ioc info <file>.ioc          # 分类概览
emb ioc get  <file>.ioc SPI6     # 查某外设全部参数
emb ioc set  <file>.ioc KEY VAL
emb ioc rm   <file>.ioc KEY
emb ioc generate <file>.ioc      # 调 CubeMX 重新生成代码
```
`.ioc` 是 ~19KB 平铺 key=value，直接读为主，emb 辅助查询。改完必须 CubeMX regenerate。
