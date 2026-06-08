---
description: Analyze a log file using the log-analyzer tool. Import logs, filter by regex, replace patterns, search for errors, and manage operation history with undo support.
---

# Analyze Log File

Use the `lga-cli` CLI tool to analyze log files. The tool stores logs in compressed repositories with full undo support.

## Available Commands

```bash
# Import a log file into a repository
lga-cli import <file> [--repo <path>]

# View repository info
lga-cli info [--repo <path>]

# View lines from current state
lga-cli view [--start N] [--count N] [--repo <path>]

# Search for lines matching a regex (read-only)
lga-cli search <pattern> [--count N] [--repo <path>]

# Filter lines by regex (keeps or removes matching lines)
lga-cli filter <pattern> [--keep/--remove] [--repo <path>]

# Replace text using regex
lga-cli replace <pattern> <replacement> [--repo <path>]

# CRUD operations on individual lines
lga-cli delete <line_indices...> [--repo <path>]
lga-cli insert <after_line> <content...> [--repo <path>]
log-analyzer modify <line_index> <new_content> [--repo <path>]

# Undo last operation
lga-cli undo [--repo <path>]

# Show operation history
lga-cli history [--repo <path>]

# Export filtered/modified log to file
lga-cli export <dest> [--repo <path>]

# Clone a repository for parallel analysis
lga-cli clone <dest> [--repo <path>]
```

## Common Workflows

### Error Investigation
```bash
lga-cli import app.log
lga-cli filter "ERROR" --keep
lga-cli filter "database" --keep
lga-cli view --count 50
```

### IP Anonymization
```bash
lga-cli import access.log
lga-cli replace '\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}' 'X.X.X.X'
lga-cli export anonymized.log
```

### Branching Analysis
```bash
lga-cli import server.log --repo main
lga-cli clone error_analysis --repo main
lga-cli clone perf_analysis --repo main
lga-cli filter "ERROR" --keep --repo error_analysis
lga-cli filter "slow|timeout" --keep --repo perf_analysis
```

## Python API

```python
from lga import LogRepo

repo = LogRepo.import_file("./repo", "server.log")
repo.filter(r"\[ERROR\]", keep=True)
lines = repo.read_all_lines()
repo.undo()
repo.export("filtered.log")
```
