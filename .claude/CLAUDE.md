# Log Graph Analyzer

A high-performance log analysis tool with a Rust backend and Python CLI frontend. Designed for analyzing text log files >10GB. Features a Git-like operation history DAG for branching, merging, and undo.

## Architecture

- **Rust core** (`src/`): Storage engine with zstd compression, line indexing, operators (filter/replace/CRUD), multi-threaded via rayon, exposed to Python via PyO3.
- **Python frontend** (`python/lograph/`): Click-based CLI with rich output.
- **Build**: maturin (pyproject.toml + Cargo.toml).

## Key Concepts

- **Log Repository**: Stores compressed log data in chunks with a line index. Git-like operation DAG for undo/redo and branching. Located in a directory (default `.logrepo/`).
- **History Tree (DAG)**: Operations form a directed acyclic graph. Each node is an operation; branches are named pointers to nodes. Supports merge, diff, and replay.
- **Operators**: Reversible transformations on log lines: filter (regex), replace (regex), delete, insert, modify. All operations record inverse data for undo.
- **Collectors**: Read-only terminal operations for aggregation: count, group-by, top-N, unique values, numeric statistics.
- **Tags**: Named line-range markers for scoped operations.

## Development

```bash
pip install -e ".[dev]"       # Build & install (includes maturin)
cargo test                     # Rust tests
pytest tests/                  # Python tests
lograph-cli --help             # CLI usage
lograph                        # Launch TUI
```

## Project Layout

```
src/                          # Rust source
├── lib.rs                    # PyO3 module entry
├── bindings.rs               # Python bindings
├── error.rs                  # Error types
├── repo/                     # Repository: storage, metadata, chunk management
├── operator/                 # Operators: filter, replace, CRUD
├── index/                    # Line indexing and chunk building
├── engine/                   # Streaming engine with Collectors
└── tui/                      # Interactive terminal UI (ratatui + crossterm)
python/lograph/               # Python package
├── __init__.py               # Re-exports from _core
└── cli.py                    # Click CLI commands
tests/                        # Rust integration + Python tests
doc/                          # Architecture, development, and design docs
```
