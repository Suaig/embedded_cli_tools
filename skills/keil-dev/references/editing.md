# 修改 .uvprojx：locate + Edit 工作流

## 为什么不直接读全文

`.uvprojx` 是 3000+ 行 XML（~128KB）。AI 一次性 Read 全文要 ~37000 token，而且后续每次 Edit 都拖着这堆上下文。实测对比：locate 定位 → Read 那几行 → Edit，上下文开销约为全读的 **1/300**。

所以默认走「定位再改」，不要 Read 整个 .uvprojx。

## locate 命令

```
emb keil locate <proj> -t <T> <element>
```
返回 `Element` / `Start Line` / `End Line` + 原始 XML 片段（raw，原样不重排）。

`element` 取值：
- `defines` —— C 编译器预处理宏节点
- `includes` —— include 路径节点
- 任意配置标签名：`Optim`（优化等级）、`ScatterFile`、`uAC6`、`CreateHexFile`、`IIncludePath2`…
- `File` —— 源文件节点（多行，start_line≠end_line）

## 三步工作流

1. **定位**：`emb keil locate <proj> -t <T> defines` → 拿到 `start_line` + raw
2. **读那几行**：Read 工具，`offset = start_line`，`limit` 按需（单行元素 1，多行看 end_line-start_line+1）
   > ⚠️ Read 的 `offset` 就是 **1-based 起始行号**，**直接用 start_line，不要减 1**。这是高频踩坑点。
3. **内容替换**：Edit 工具，`old_string` 用读到的原始文本（保证唯一），`new_string` 是改后的。

### 例子：加一个预处理宏

```
emb keil locate app.uvprojx -t STM32H723 defines
# → Start Line: 341
# → <Define>USE_HAL_DRIVER,STM32H723xx,USE_KF</Define>
```
Read(offset=341, limit=1) → Edit：把 `USE_KF</Define>` 替换成 `USE_KF,NEW_MACRO</Define>`。

## editor 命令（增删整节点才用）

加/删源文件、加/删分组这类「整节点增删」用 editor 命令（见 commands.md）。改单个值（宏、优化等级、scatter）不要用 editor，用上面的 locate+Edit。

### editor 的格式代价（已大幅收敛）

editor 底层用 xmltree 全量解析再序列化，**任何一次 editor 写操作都会重写整个 `.uvprojx`**，DOM 会丢掉一些格式。`save_xml` 事后会从原文把这几项补回来：

| 行为 | 现在 |
|---|---|
| XML 声明 `standalone="no"` | 已还原 |
| `xsi:` 命名空间前缀 | 已还原（直接沿用原文的根元素开标签） |
| 空元素风格 | 按原文件的习惯逐个还原（Keil 同一文件里 `<Tag></Tag>` 和 `<Tag/>` 混用） |
| 结尾换行 | 已还原 |
| 空行 | **仍会丢**（DOM 不记录位置） |
| 属性顺序 | **仍会被打乱**（DOM 用哈希存属性，顺序不可恢复） |

实测（2026-09-18，1836 行的工程做一次净零改动：加一个宏再删掉）：**10 行**发生变化 —— 6 行是丢失的空行，4 行来自 2 个属性顺序被打乱的元素。补丁前是 1508 行。

所以现在的代价是"几十行以内的可读 diff"，不再是"整文件重写"。editor 会在同目录留一个覆盖式的 `.uvprojx.bak`（只保留上一次，不累积）。

**仍建议**：改单个值优先走 locate+Edit（真正的零改动）；增删节点用 editor，改完 `git diff` 扫一眼。

## 多 target 工程

`-t <T>` 选 target。多 target 工程的 .uvprojx 有多个 `<Target>`，locate/editor 都按指定 target 操作，不会误伤别的 target。
