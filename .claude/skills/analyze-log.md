---
description: Analyze a log file using Log Graph Analyzer. Import logs, filter by regex, replace patterns, search for errors, and manage operation history with undo support.
---

# Analyze Log File

Use the `lograph-cli` CLI tool to analyze log files. The tool stores logs in compressed repositories with full undo support.

## Available Commands

```bash
# Import a log file into a repository
lograph-cli import <file> [--repo <path>]

# View repository info
lograph-cli info [--repo <path>]

# View lines from current state
lograph-cli view [--start N] [--count N] [--repo <path>]

# Search for lines matching a regex (read-only)
lograph-cli search <pattern> [--count N] [--repo <path>]

# Filter lines by regex (keeps or removes matching lines)
lograph-cli filter <pattern> [--keep/--remove] [--repo <path>]

# Replace text using regex
lograph-cli replace <pattern> <replacement> [--repo <path>]

# CRUD operations on individual lines
lograph-cli delete <line_indices...> [--repo <path>]
lograph-cli insert <after_line> <content...> [--repo <path>]
lograph-cli modify <line_index> <new_content> [--repo <path>]

# Undo last operation
lograph-cli undo [--repo <path>]

# Show operation history
lograph-cli history [--repo <path>]

# Export filtered/modified log to file
lograph-cli export <dest> [--repo <path>]

# Clone a repository for parallel analysis
lograph-cli repo clone <src> <dst> [--workspace <path>]
```

## Common Workflows

### Error Investigation
```bash
lograph-cli import app.log
lograph-cli filter "ERROR" --keep
lograph-cli filter "database" --keep
lograph-cli view --count 50
```

### IP Anonymization
```bash
lograph-cli import access.log
lograph-cli replace '\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}' 'X.X.X.X'
lograph-cli export anonymized.log
```

### Branching Analysis
```bash
lograph-cli import server.log --repo main
lograph-cli repo clone main error_analysis
lograph-cli repo clone main perf_analysis
lograph-cli filter "ERROR" --keep --repo error_analysis
lograph-cli filter "slow|timeout" --keep --repo perf_analysis
```

## Python API

```python
from lograph import Workspace

ws = Workspace(".logrepo")
ws.import_file("server.log", "my_repo")
repo = ws.open_repo("my_repo")
repo.filter(r"\[ERROR\]", keep=True)
lines = repo.read_all_lines()
repo.undo()
repo.export("filtered.log")
```
