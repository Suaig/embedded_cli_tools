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

### ⚠️ editor 的格式代价

editor 底层用 xmltree 读写 XML，有三个已知行为：
- **重排属性顺序**（按字母序）
- **丢 `xsi:` 命名空间前缀**（如 `xsi:schemaLocation`）
- **删空行**

Keil 能正常加载这些被改过的文件（编译不受影响），但 `git diff` 会产生几百行噪音（属性移动、空行消失）。

**所以**：改值 → locate+Edit（零破坏）；增删节点 → editor（改完用 `git diff` 检查，确认能接受）。

## 多 target 工程

`-t <T>` 选 target。多 target 工程的 .uvprojx 有多个 `<Target>`，locate/editor 都按指定 target 操作，不会误伤别的 target。
