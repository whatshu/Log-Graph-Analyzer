# Development Guide

## Prerequisites

- **Python >= 3.10** (for the CLI frontend and Python bindings)
- **Rust toolchain** (rustc + cargo, install via [rustup](https://rustup.rs))
- **maturin** (installed automatically by build script)

## Setup

```bash
# Clone and enter the project
git clone <repo-url> && cd log-analyzer

# Editable development install (recommended)
pip install -e ".[dev]"

# Or use the build script
./build.sh install --dev
```

## Building

```bash
./build.sh                    # Build release wheel only (no install)
./build.sh --dev              # Build debug wheel only
./build.sh install            # Build release wheel and install
./build.sh install --dev      # Editable development install (rebuilds on Rust changes)
./build.sh uninstall          # Remove installed package
./build.sh test               # Install and run full test suite
```

Or manually:

```bash
pip install -e ".[dev]"       # Editable install (uses maturin)
maturin build --release       # Build .whl to target/wheels/
cargo build --bin lograph --no-default-features --release  # TUI only
```

## Project Structure

```
log-analyzer/
├── build.sh                  Build/install/uninstall script
├── Cargo.toml                Rust package config
├── pyproject.toml            Python package config (maturin)
│
├── src/                      Rust core
│   ├── lib.rs                PyO3 module entry
│   ├── main.rs               TUI binary entry
│   ├── bindings.rs           Python class/method bindings
│   ├── error.rs              Error types
│   ├── config.rs             Multi-tier configuration
│   ├── cache.rs              Node state caching (LRU + zstd)
│   ├── history.rs            History tree (DAG) + serialization
│   ├── tag.rs                Tag store for scoped operations
│   ├── repo/                 Log repository
│   │   ├── mod.rs            LogRepo: create, open, append, operations, undo
│   │   ├── workspace.rs      Workspace: named repo management, clone, merge
│   │   ├── storage.rs        ChunkStorage: zstd compressed chunk I/O
│   │   └── metadata.rs       RepoMetadata: UUID, timestamps, stats
│   ├── index/                Line indexing
│   │   ├── mod.rs            LineIndex: chunk-based line lookup
│   │   └── builder.rs        IndexBuilder: parallel newline scanning
│   ├── operator/             Reversible operators
│   │   ├── mod.rs            Operation enum, InverseData, dispatch
│   │   ├── filter.rs         Regex filter (keep/remove)
│   │   ├── replace.rs        Regex replace with capture groups
│   │   └── crud.rs           DeleteLines, InsertLines, ModifyLine
│   ├── engine/               Streaming processing engine
│   │   ├── mod.rs            Shared chunk reading utilities
│   │   ├── fast.rs           ripgrep-powered SIMD search (grep-searcher)
│   │   ├── stream.rs         LineStream: chunk-by-chunk iterator
│   │   ├── processor.rs      ChunkedProcessor: streaming filter/replace/search
│   │   └── collector.rs      Collector: count, group_count, top_n, unique, numeric_stats
│   └── tui/                  Terminal UI
│       ├── mod.rs            TUI entry + event loop
│       ├── app.rs            Application state
│       ├── ui.rs             Rendering
│       ├── event.rs          Terminal setup/restore
│       ├── file_browser.rs   File browser component
│       ├── handlers/         Key handlers
│       │   ├── mod.rs        Normal/command/search/input mode handlers
│       │   └── ops.rs        History node operations
│       └── widgets/          TUI widgets
│
├── python/lograph/           Python package
│   ├── __init__.py           Re-exports from _core
│   └── cli.py                Click CLI
│
├── doc/                      Documentation
│   ├── architecture.md       System architecture
│   ├── development.md        This guide
│   ├── benchmarks.md         Performance benchmarks
│   └── design.md             Design decisions
│
├── tests/                    Test suite
├── benchmarks/               Performance benchmarks
├── scripts/                  Utility scripts
└── .claude/                  AI agent configuration
```

## Testing

```bash
cargo test                  # Rust tests
pytest tests/ -v            # Python tests
./build.sh test             # Build, install, and run all tests
```

## Workspace Layout on Disk

```
.logrepo/                       Workspace root
├── workspace.json              Active repo tracker: {"active": "default"}
├── default/                    Named repository
│   ├── meta.json               Repository metadata
│   ├── index.json              Line index (chunk boundaries)
│   ├── chunks/                 Compressed data chunks
│   │   ├── 000000.zst
│   │   ├── 000001.zst
│   │   └── ...
│   └── operations.json         Operation journal (history DAG)
├── error_analysis/             Cloned repository
│   └── ...
└── ...
```

## Distribution

### Building a Wheel

```bash
./build.sh                  # or: maturin build --release
```

Produces a `.whl` in `target/wheels/` for the current platform.

### Cross-Platform Wheels

Use maturin with Docker for manylinux builds:

```bash
docker run --rm -v $(pwd):/io ghcr.io/pyo3/maturin build --release
maturin build --release --target aarch64-unknown-linux-gnu
```

### Publishing to PyPI

```bash
maturin publish
maturin publish --repository testpypi   # Test first on TestPyPI
```

### Cargo Install

```bash
# Install the TUI binary
cargo install lograph --no-default-features

# Then run
lograph -w .logrepo -r myrepo
```

### Platform Matrix

| Platform | Target |
|----------|--------|
| Linux x86_64 | `x86_64-unknown-linux-gnu` (manylinux) |
| Linux aarch64 | `aarch64-unknown-linux-gnu` |
| macOS x86_64 | `x86_64-apple-darwin` |
| macOS ARM | `aarch64-apple-darwin` |
| Windows x86_64 | `x86_64-pc-windows-msvc` |
