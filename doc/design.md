# Design Decisions

## Why Rust + Python?

**Rust** provides the performance-critical core — chunk compression, regex matching, parallel processing. The borrow checker prevents data races in multi-threaded code (rayon).

**Python** provides the user-facing CLI — click for argument parsing, rich for terminal output. PyO3 bridges the two with zero-copy where possible.

This split avoids the "two-language problem" for users: the performance-critical path is Rust, the scripting/automation path is Python.

## Why a Git-like History DAG?

Traditional log analysis tools are stateless: each query starts from scratch. By storing operations as a DAG:

1. **Undo is free** — just move a pointer
2. **Branching enables parallel exploration** — no need to re-import for each analysis angle
3. **Merge enables combining results** — union of filtered line sets
4. **Diff enables comparison** — "what lines did filter A remove that filter B kept?"

The DAG was chosen over a flat linear history because log analysis often involves divergent exploration paths ("what if I filter for errors?" vs "what if I filter for slow requests?").

## Why Zstd Compression?

- **Compression ratio**: 2.1–2.5× on typical log files (tested on JSON, syslog, and access logs)
- **Decompression speed**: ~500 MB/s, fast enough to not bottleneck line iteration
- **Dictionary training**: Optional, for even better ratios on structured logs

Alternatives considered:
- **Snappy**: Faster but 1.5–1.8× compression ratio (rejected for space efficiency)
- **LZ4**: Similar speed to zstd but worse ratio (rejected)
- **Gzip**: Better ratio than Snappy but much slower decompression (rejected)

## Why ripgrep's grep-searcher?

For regex matching, using ripgrep's `grep-searcher` crate provides:
- **SIMD literal acceleration**: uses memchr for multi-pattern literal matching
- **Zero-copy**: operates on `&[u8]` without allocating
- **Battle-tested**: ripgrep is the gold standard for text search performance

## Why Ratatui + Crossterm for the TUI?

- **Ratatui**: Immediate-mode rendering with layout constraints
- **Crossterm**: Cross-platform terminal manipulation (raw mode, alternate screen, events)
- Together they provide a responsive, vim-like TUI experience with minimal dependencies

## Tag System Design

Tags operate on line ranges rather than pattern matching because:
1. **Deterministic**: line ranges are stable (patterns can match different lines after edits)
2. **Visual**: users can see exactly what lines are tagged
3. **Scoped ops**: tag scopes apply to all subsequent operations, not just one

The persist-to-disk approach (JSON in `.log_analyzer/tags.json`) was chosen over embedding in the DAG to keep tag management lightweight and session-independent.
