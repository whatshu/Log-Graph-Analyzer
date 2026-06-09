# Log Graph Analyzer

[English](README.md)

**lograph 是 Log Graph Analyzer 的命令行界面。** —
一个高性能日志分析工具，将日志数据视为操作图谱，支持可逆过滤、分支分析和撤销操作。专为 **10 GB 以上**的文本日志文件设计。

## 什么是 Log Graph Analyzer？

Log Graph Analyzer 帮助你交互式地探索和分析大型日志文件。不再需要运行一次性的 `grep` 命令——只需**导入**一次日志文件，就可以**过滤**、**搜索**、**替换**和**统计**，并支持完整的**撤销**和**分支**功能。可以把它看作是"日志版的 Git"。

- 🗜️ **压缩存储** — 使用 zstd 将日志压缩至原始大小的 40%
- 🌳 **历史图谱** — 每个操作都是 DAG 中的一个节点；支持分支、合并和对比
- ↩️ **无限撤销** — 所有操作都可逆
- 📊 **内置分析** — 计数、分组、Top-N、去重、数值统计
- ⚡ **快速搜索** — 基于 ripgrep 的 SIMD 加速搜索引擎
- 🖥️ **TUI + CLI** — 交互式终端界面或可脚本化的命令行
- 🐍 **Python API** — 可在自己的脚本和 notebook 中作为库使用

## 快速开始

### 安装

```bash
# 通过 pip 安装（包含 lograph-cli 和 Python 库）
pip install lograph

# 或使用 cargo 仅安装 TUI 二进制
cargo install lograph --no-default-features
```

### 首次分析

```bash
# 导入日志文件
lograph-cli import server.log

# 查看前 20 行
lograph-cli view

# 统计所有 ERROR 行
lograph-cli stats count ERROR

# 过滤保留 ERROR 行
lograph-cli filter ERROR --keep

# 撤销过滤
lograph-cli undo

# 导出当前状态
lograph-cli export filtered.log
```

### 使用 TUI

```bash
# 启动交互式终端界面
lograph

# 或指定工作区和仓库
lograph -w .logrepo -r myrepo
```

在 TUI 中按 `?` 查看所有快捷键。

## 功能特性

### Git 风格的历史图谱

每个操作都会成为历史图谱中的一个节点。你可以：

- **分支** — 从任意节点分支出独立的分析路径
- **合并** — 合并节点以组合过滤结果
- **对比** — 比较两个节点的差异
- **撤销** — 在图中向后移动以撤销操作

### 收集器（内置分析）

受 Java Stream Collectors 启发的只读聚合操作：

| 收集器 | 描述 | 示例 |
|--------|------|------|
| `count` | 计数匹配行 | `stats count ERROR` |
| `group-count` | 按捕获组分组 | `stats group-count '\[(\w+)\]'` |
| `top` | Top-N 频率 | `stats top 'clientId=(\d+)' -n 10` |
| `distinct` | 去重值 | `stats distinct 'src=(\S+)'` |
| `numbers` | 数值统计 (min/max/avg/sum) | `stats numbers 'latency=(\d+)ms'` |

### 标签系统

用命名标签标记行范围，实现局部操作。可仅在标记区域内进行过滤、搜索和统计。

### 流式引擎

逐块处理 10 GB 以上的文件，无需将全部数据加载到内存。可在单遍流式处理中过滤、搜索或收集统计信息。

### 四种使用方式

1. **`lograph`** — 交互式终端界面（ratatui + crossterm）
2. **`lograph-cli`** — 用于脚本的命令行界面
3. **Python 库** — `from lograph import Workspace, LogRepo`
4. **Rust 库** — 在 Cargo.toml 中添加 `lograph = "0.0.1"`（禁用默认 features）

## CLI 命令参考

### 日志操作

| 命令 | 说明 |
|------|------|
| `import <file>` | 导入文本文件到新仓库 |
| `append <file>` | 向现有仓库追加文本文件 |
| `info` | 显示仓库元信息和操作数 |
| `view` | 查看当前状态的日志行 |
| `search <pattern>` | 搜索匹配正则的行（只读） |
| `filter <pattern>` | 保留（`--keep`）或移除（`--remove`）匹配行 |
| `replace <pattern> <replacement>` | 正则替换（支持捕获组） |
| `delete <indices...>` | 按索引删除行 |
| `insert <after> <content...>` | 在指定位置后插入行 |
| `modify <index> <content>` | 替换单行内容 |
| `undo` | 撤销上一个操作 |
| `history` | 显示操作日志 |
| `export <file>` | 将当前状态写入文件 |

### 仓库管理

| 命令 | 说明 |
|------|------|
| `repo list` | 列出所有仓库（`*` 标记当前活跃） |
| `repo use <name>` | 切换活跃仓库 |
| `repo clone <src> <dst>` | 按名称克隆仓库 |
| `repo remove <name>` | 删除仓库 |

### 分析统计 (stats)

| 命令 | 说明 |
|------|------|
| `stats overview` | 概览统计 |
| `stats count [pattern]` | 计数（可选过滤） |
| `stats group-count <p>` | 按捕获组分组 |
| `stats top <p>` | Top-N 频率 |
| `stats distinct <p>` | 去重值 |
| `stats numbers <p>` | 数值统计 |

### 其他

| 命令 | 说明 |
|------|------|
| `branch list/create/checkout/delete` | 管理分析分支 |
| `node merge/subtract/delete` | 历史节点操作 |
| `tag list/create/delete/rename` | 标签管理 |
| `merge <srcs> --into <tgt>` | 合并多个仓库 |
| `search-file <file> <p>` | 直接搜索文件（无需导入） |

## Python API

```python
from lograph import Workspace

# 打开工作区
ws = Workspace(".logrepo")

# 导入日志文件
ws.import_file("server.log", "my_repo")

# 打开并分析
repo = ws.open_repo("my_repo")

# 统计
errors = repo.collect_count("ERROR")
levels = repo.collect_group_count(r"\[(\w+)\]", 1)
top_clients = repo.collect_top_n(r"clientId=(\d+)", 1, 10)
latency_stats = repo.collect_numeric_stats(r"latency=(\d+)ms", 1)

# 可逆操作
repo.filter(r"\[ERROR\]", keep=True)
repo.replace(r"\d{4}-\d{2}-\d{2}", "DATE")
repo.undo()

# 导出
repo.export("output.log")
```

## 性能

Log Graph Analyzer 在性能上与传统命令行工具相当或更优。在压缩仓库上重复查询时，可比 ripgrep 快 **2.3 倍**。

详见 [doc/benchmarks.md](doc/benchmarks.md)，其中对比了 lograph 与 grep、ripgrep、sed、awk 和 Python 在 10 GiB 测试文件上的详细基准测试。

## 项目状态

Log Graph Analyzer 正在活跃开发中。核心引擎稳定且经过充分测试。新功能、平台支持和性能改进正在进行中。

## 延伸阅读

- [架构](doc/architecture.md) — 系统架构与数据流
- [开发指南](doc/development.md) — 项目结构、构建和分发
- [设计决策](doc/design.md) — 我们做出这些选择的原因
- [性能基准](doc/benchmarks.md) — 详细的性能测量

## 许可证

MIT
