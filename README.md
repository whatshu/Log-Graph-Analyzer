# Log Graph Analyzer

[中文文档](README_zh.md)

**lograph is the command-line interface of Log Graph Analyzer.** —
A high-performance log analysis tool that treats your log data as a graph of operations, enabling reversible filtering, branching analysis, and undo support. Designed for text log files exceeding **10 GB**.

## What is Log Graph Analyzer?

Log Graph Analyzer helps you explore and analyze large log files interactively. Instead of running throwaway `grep` commands, you **import** a log file once, then **filter**, **search**, **replace**, and **collect statistics** — with full **undo** and **branching** support. Think of it as "Git for logs."

- 🗜️ **Compressed storage** — logs compress to 40% of original size with zstd
- 🌳 **History graph** — every operation is a node in a DAG; branch, merge, and diff
- ↩️ **Unlimited undo** — all operations are reversible
- 📊 **Built-in analytics** — count, group, top-N, unique values, numeric stats
- ⚡ **Fast search** — powered by ripgrep's SIMD-accelerated search engine
- 🖥️ **TUI + CLI** — interactive terminal interface or scriptable command line
- 🐍 **Python API** — use as a library in your own scripts and notebooks

## Quick Start

### Install

```bash
# Via pip (includes both lograph-cli and Python library)
pip install lograph

# Or use cargo for the TUI binary only
cargo install lograph --no-default-features
```

### First Analysis

```bash
# Import a log file
lograph-cli import server.log

# View the first 20 lines
lograph-cli view

# Count all ERROR lines
lograph-cli stats count ERROR

# Filter to keep only ERROR lines
lograph-cli filter ERROR --keep

# Undo that filter
lograph-cli undo

# Export the current state
lograph-cli export filtered.log
```

### Using the TUI

```bash
# Launch the interactive terminal UI
lograph

# Or specify workspace and repo
lograph -w .logrepo -r myrepo
```

In the TUI, press `?` to see all keybindings.

## Features

### Git-like History Graph

Every operation you apply becomes a node in a history graph. You can:

- **Branch** from any node to explore different analysis paths
- **Merge** nodes to combine filtered results
- **Diff** nodes to see what one filter removed that another kept
- **Undo** any operation by moving back in the graph

### Collectors (Built-in Analytics)

Read-only aggregations inspired by Java Stream Collectors:

| Collector | Description | Example |
|-----------|-------------|---------|
| `count` | Count matching lines | `stats count ERROR` |
| `group-count` | Group by capture group | `stats group-count '\[(\w+)\]'` |
| `top` | Top-N most frequent values | `stats top 'clientId=(\d+)' -n 10` |
| `distinct` | Unique values of a group | `stats distinct 'src=(\S+)'` |
| `numbers` | Numeric stats (min/max/avg/sum) | `stats numbers 'latency=(\d+)ms'` |

### Tag System

Mark line ranges with named tags for scoped operations. Filter, search, and collect only within tagged regions.

### Streaming Engine

Process files >10 GB chunk-by-chunk without loading everything into memory. Filter to file, search, or collect statistics in a single streaming pass.

### Four Ways to Use

1. **`lograph`** — Interactive terminal UI (ratatui + crossterm)
2. **`lograph-cli`** — Command-line interface for scripting
3. **Python library** — `from lograph import Workspace, LogRepo`
4. **Rust library** — `lograph = "0.0.1"` in Cargo.toml (disable default features)

## CLI Reference

### Log Operations

| Command | Description |
|---------|-------------|
| `import <file>` | Import a text file into a new repository |
| `append <file>` | Append a text file into an existing repository |
| `info` | Show repository metadata and operation count |
| `view` | View lines from the current state |
| `search <pattern>` | Search for regex matches (read-only) |
| `filter <pattern>` | Keep (`--keep`) or remove (`--remove`) matching lines |
| `replace <pattern> <replacement>` | Regex replace with capture groups |
| `delete <indices...>` | Delete lines by index |
| `insert <after> <content...>` | Insert lines after a position |
| `modify <index> <content>` | Replace a single line |
| `undo` | Undo the last operation |
| `history` | Show the operation journal |
| `export <file>` | Write the current state to a file |

### Repository Management

| Command | Description |
|---------|-------------|
| `repo list` | List all repos (`*` marks active) |
| `repo use <name>` | Switch the active repository |
| `repo clone <src> <dst>` | Clone a repo under a new name |
| `repo remove <name>` | Delete a repository |

### Analytics (stats)

| Command | Description |
|---------|-------------|
| `stats overview` | Overview statistics |
| `stats count [pattern]` | Count lines (optionally filtered) |
| `stats group-count <p>` | Group by capture group |
| `stats top <p>` | Top-N frequent values |
| `stats distinct <p>` | Distinct values |
| `stats numbers <p>` | Numeric statistics |

### Other

| Command | Description |
|---------|-------------|
| `branch list/create/checkout/delete` | Manage analysis branches |
| `node merge/subtract/delete` | History node operations |
| `tag list/create/delete/rename` | Tag management |
| `merge <srcs> --into <tgt>` | Merge multiple repos |
| `search-file <file> <p>` | Search a file directly (no import) |

## Python API

```python
from lograph import Workspace

# Open workspace
ws = Workspace(".logrepo")

# Import a log file
ws.import_file("server.log", "my_repo")

# Open and analyze
repo = ws.open_repo("my_repo")

# Collect statistics
errors = repo.collect_count("ERROR")
levels = repo.collect_group_count(r"\[(\w+)\]", 1)
top_clients = repo.collect_top_n(r"clientId=(\d+)", 1, 10)
latency_stats = repo.collect_numeric_stats(r"latency=(\d+)ms", 1)

# Reversible operations
repo.filter(r"\[ERROR\]", keep=True)
repo.replace(r"\d{4}-\d{2}-\d{2}", "DATE")
repo.undo()

# Export
repo.export("output.log")
```

## Performance

Log Graph Analyzer achieves competitive or superior performance compared to classic command-line tools. On repeated queries against compressed repositories, it is up to **2.3× faster than ripgrep**.

See [doc/benchmarks.md](doc/benchmarks.md) for detailed benchmarks comparing lograph against grep, ripgrep, sed, awk, and Python on a 10 GiB test file.

## Project Status

Log Graph Analyzer is under active development. The core engine is stable and well-tested. New features, platform support, and performance improvements are ongoing.

## Further Reading

- [Architecture](doc/architecture.md) — System architecture and data flow
- [Development Guide](doc/development.md) — Project structure, building, and distribution
- [Design Decisions](doc/design.md) — Why we made the choices we did
- [Benchmarks](doc/benchmarks.md) — Detailed performance measurements

## License

MIT
