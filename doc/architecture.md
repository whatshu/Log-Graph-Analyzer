# Architecture

Log Graph Analyzer (lograph) is built as a hybrid Rust-Python application with a Git-like history graph at its core.

## Overview

```
┌─────────────────────────────────────────────────┐
│                  User Interface                  │
├──────────────────┬──────────────────────────────┤
│   lograph (TUI)  │    lograph-cli (Python CLI)  │
│  ratatui+tui-rs  │      click + rich            │
├──────────────────┴──────────────────────────────┤
│              Python Bindings (PyO3)              │
├─────────────────────────────────────────────────┤
│                 Rust Core Engine                 │
│  ┌──────────┬──────────┬──────────┬──────────┐ │
│  │  Repo    │ Operator │  Engine  │  Index   │ │
│  │ Storage  │ Filter/  │ Stream/  │ Builder  │ │
│  │ Workspace│ Replace  │ Collector│          │ │
│  └──────────┴──────────┴──────────┴──────────┘ │
│  ┌────────────────────────────────────────────┐ │
│  │         History Tree (DAG)                  │ │
│  │   Git-like branching + operation journal    │ │
│  └────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────┘
```

## Core Components

### Log Repository (Repo)

A log repository stores compressed log data in chunks with a line index. Each repo is a directory containing:

- `meta.json` — Metadata (UUID, source file, line count, size)
- `index.json` — Line index mapping chunk boundaries to byte offsets
- `chunks/` — Zstd-compressed data chunks (e.g., `000000.zst`)
- `operations.json` — Operation journal for undo/redo

Chunks are typically 64 KB each, providing fine-grained random access without decompressing the entire file.

### History Tree (DAG)

The operation history is stored as a directed acyclic graph (DAG), similar to Git's commit graph:

- Each **node** represents one operation (filter, replace, delete, etc.)
- The **root node** (id=0) represents the initial import
- **Branches** are named pointers to specific nodes
- Operations can branch from any node, creating a tree structure
- Nodes support **soft-delete** (marked deleted, pattern preserved)

This enables:
- **Undo**: Move branch HEAD back to parent node
- **Branching**: Create independent analysis branches
- **Merge**: Union of line sets from multiple nodes
- **Diff**: Set subtraction between two nodes

### Operators

All operations are reversible. Each operation records inverse data:

| Operator | Forward | Inverse |
|----------|---------|---------|
| Filter | Keep/remove matching lines | Restore removed lines |
| Replace | Regex substitution | Save originals, restore on undo |
| DeleteLines | Remove by index | Save removed lines |
| InsertLines | Insert at position | Record position + count |
| ModifyLine | Replace single line | Save original content |

### Streaming Engine

For files >10 GB, loading all data into memory is impractical. The streaming engine processes data chunk-by-chunk:

- **LineStream**: Iterates over lines across chunks
- **ChunkedProcessor**: Applies filter/replace/search in streaming fashion
- **Collector**: Aggregates (count, group-by, top-N, unique, numeric stats) in a single pass
- **Fast Search**: Uses ripgrep's `grep-searcher` with SIMD literal acceleration

### Workspace

A workspace manages multiple named repositories in a shared directory (default `.logrepo/`). Similar to a Git repository with multiple worktrees:

```
.logrepo/
├── workspace.json    # Active repo tracker
├── repo_a/
│   ├── meta.json
│   ├── index.json
│   ├── chunks/
│   └── operations.json
├── repo_b/
│   └── ...
└── ...
```

### TUI (Terminal UI)

Built with ratatui + crossterm, providing:

- **Log viewer** with line numbers, cursor navigation, and search highlighting
- **History tree** visualizer with git-like graph connectors
- **Repository browser** for switching between repos
- **File browser** for importing new log files
- **Command mode** with ex-style commands (`:f`, `:r`, `:w`, etc.)
- **Tag manager** for scoped line-range operations
- **Collect popup** for statistical analysis results
- **Tmux integration** with window title sync
- **ASCII fallback** for terminals without UTF-8 support

### Python CLI

A Click-based CLI frontend providing all operations from the command line. Features rich terminal output via the `rich` library.

### Tag System

Tags mark specific line ranges for scoped operations. When a tag scope is active, all subsequent operations only apply to lines within that range. Tags persist across sessions in JSON format.

## Data Flow

```
Import:  Raw File → IndexBuilder (parallel) → Chunks (zstd) + Index
View:    Index → Chunk Lookup → Decompress → Return Lines
Filter:  Current State → Apply Regex → Save Inverse → Return New State
Undo:    History DAG → Move HEAD Back → Reconstruct Previous State
Collect: Original Data → Stream Chunks → Aggregate → Return Result
```
