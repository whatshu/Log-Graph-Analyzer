"""CLI frontend for lograph-cli — Log Graph Analyzer."""

import os
import sys

import click
from rich.console import Console
from rich.table import Table

from lograph._core import LogRepo, TagStore, Workspace

console = Console()

DEFAULT_WORKSPACE = ".logrepo"


def get_workspace(workspace: str | None = None) -> Workspace:
    """Open workspace, auto-migrating old flat layout if needed."""
    root = workspace or DEFAULT_WORKSPACE
    return Workspace(root)


def open_repo(workspace: str | None, repo: str | None):
    """Open a named repo from the workspace."""
    ws = get_workspace(workspace)
    if not ws.is_initialized():
        console.print("[red]No workspace found. Use 'import' to create one.[/red]")
        sys.exit(1)
    name = repo or ws.active()
    return ws.open_repo(name)


# ---------------------------------------------------------------------------
# Main group
# ---------------------------------------------------------------------------

@click.group()
@click.version_option(version="0.1.0")
def main():
    """lograph-cli — Log Graph Analyzer: high-performance log analysis for large text files.

    Stores logs in compressed repositories with Git-like history DAG
    and undo support. Designed for files >10GB.

    Use --repo NAME to target a specific repo (default: active repo).
    Use 'repo' subcommand to manage repos (list, clone, remove, use).
    """
    pass


# ---------------------------------------------------------------------------
# repo subcommand group
# ---------------------------------------------------------------------------

@main.group()
def repo():
    """Manage repositories (list, clone, remove, use)."""
    pass


@repo.command(name="list")
@click.option("--workspace", "-w", default=None, help="Workspace directory")
def repo_list(workspace: str | None):
    """List all repositories in the workspace."""
    ws = get_workspace(workspace)
    if not ws.is_initialized():
        console.print("[dim]No workspace found.[/dim]")
        return

    active = ws.active()
    repos = ws.list()

    if not repos:
        console.print("[dim]No repositories.[/dim]")
        return

    table = Table(title="Repositories")
    table.add_column("", style="cyan", width=3)
    table.add_column("Name", style="green")
    table.add_column("Lines", justify="right")
    table.add_column("Source")

    for name in repos:
        try:
            r = ws.open_repo(name)
            meta = r.metadata()
            marker = "*" if name == active else ""
            table.add_row(marker, name, f"{meta.original_line_count:,}", meta.source_name)
        except Exception:
            table.add_row("", name, "?", "?")

    console.print(table)
    console.print(f"\n[dim]Active: {active}[/dim]")


@repo.command(name="use")
@click.argument("name")
@click.option("--workspace", "-w", default=None, help="Workspace directory")
def repo_use(name: str, workspace: str | None):
    """Switch the active repository."""
    ws = get_workspace(workspace)
    ws.set_active(name)
    console.print(f"[green]Active repo:[/green] {name}")


@repo.command(name="clone")
@click.argument("src")
@click.argument("dst")
@click.option("--workspace", "-w", default=None, help="Workspace directory")
def repo_clone(src: str, dst: str, workspace: str | None):
    """Clone a repository under a new name."""
    ws = get_workspace(workspace)
    with console.status(f"Cloning {src} -> {dst}..."):
        ws.clone_repo(src, dst)
    console.print(f"[green]Cloned:[/green] {src} -> {dst}")


@repo.command(name="remove")
@click.argument("name")
@click.option("--workspace", "-w", default=None, help="Workspace directory")
@click.option("--yes", "-y", is_flag=True, help="Skip confirmation")
def repo_remove(name: str, workspace: str | None, yes: bool):
    """Remove a repository."""
    ws = get_workspace(workspace)
    if not yes:
        click.confirm(f"Remove repo '{name}'? This cannot be undone", abort=True)
    ws.remove_repo(name)
    console.print(f"[green]Removed:[/green] {name}")


# ---------------------------------------------------------------------------
# branch subcommand group
# ---------------------------------------------------------------------------

@main.group()
def branch():
    """Manage branches (list, checkout, create, delete)."""
    pass


@branch.command(name="list")
@click.option("--repo", "-r", default=None, help="Repository name")
@click.option("--workspace", "-w", default=None, help="Workspace directory")
def branch_list(repo: str | None, workspace: str | None):
    """List all branches in the current repository."""
    log_repo = open_repo(workspace, repo)

    branches = log_repo.branch_names()
    current = log_repo.current_branch_name()
    if not branches:
        console.print("[dim]No branches (only 'main' exists).[/dim]")
        return

    table = Table(title="Branches")
    table.add_column("", width=3)
    table.add_column("Name", style="green")
    table.add_column("HEAD Node", style="cyan", justify="right")

    for b in branches:
        b_head = log_repo.branch_head_node_id(b)
        marker = "*" if b == current else ""
        table.add_row(marker, b, str(b_head) if b_head is not None else "?")

    console.print(table)
    console.print(f"\n[dim]Current branch: {current}[/dim]")


@branch.command(name="checkout")
@click.argument("name")
@click.option("--repo", "-r", default=None, help="Repository name")
@click.option("--workspace", "-w", default=None, help="Workspace directory")
def branch_checkout(name: str, repo: str | None, workspace: str | None):
    """Switch to a named branch."""
    log_repo = open_repo(workspace, repo)
    log_repo.checkout_branch(name)
    console.print(f"[green]Switched to branch:[/green] {name}")


@branch.command(name="create")
@click.argument("name")
@click.option("--at", "-a", type=int, default=None, help="Node ID to branch from (default: current HEAD)")
@click.option("--repo", "-r", default=None, help="Repository name")
@click.option("--workspace", "-w", default=None, help="Workspace directory")
def branch_create(name: str, at: int | None, repo: str | None, workspace: str | None):
    """Create a new branch at a given history node."""
    log_repo = open_repo(workspace, repo)
    node_id = at if at is not None else log_repo.head_node_id()
    created = log_repo.create_branch(name, node_id)
    if created:
        console.print(f"[green]Branch '{name}' created at node {node_id}[/green]")
    else:
        console.print(f"[red]Branch '{name}' already exists.[/red]")


@branch.command(name="delete")
@click.argument("name")
@click.option("--repo", "-r", default=None, help="Repository name")
@click.option("--workspace", "-w", default=None, help="Workspace directory")
@click.option("--yes", "-y", is_flag=True, help="Skip confirmation")
def branch_delete(name: str, repo: str | None, workspace: str | None, yes: bool):
    """Delete a branch. Cannot delete 'main' or the current branch."""
    log_repo = open_repo(workspace, repo)
    if name == "main":
        console.print("[red]Cannot delete 'main' branch.[/red]")
        return
    if name == log_repo.current_branch_name():
        console.print("[red]Cannot delete the current branch. Switch to another first.[/red]")
        return
    if not yes:
        click.confirm(f"Delete branch '{name}'?", abort=True)
    deleted = log_repo.delete_branch(name)
    if deleted:
        console.print(f"[green]Branch '{name}' deleted.[/green]")
    else:
        console.print(f"[red]Branch '{name}' not found.[/red]")


# ---------------------------------------------------------------------------
# Top-level log commands (operate on the active or --repo repo)
# ---------------------------------------------------------------------------

@main.command(name="import")
@click.argument("file", type=click.Path(exists=True))
@click.option("--repo", "-r", default="default", help="Repository name")
@click.option("--workspace", "-w", default=None, help="Workspace directory")
def import_cmd(file: str, repo: str, workspace: str | None):
    """Import a text file into a new repository."""
    ws = get_workspace(workspace)
    with console.status("Importing log file..."):
        log_repo = ws.import_file(file, repo)

    meta = log_repo.metadata()
    console.print(f"[green]Repo '{repo}' created[/green]")
    console.print(f"  Source: {meta.source_name}")
    console.print(f"  Lines:  {meta.original_line_count:,}")
    console.print(f"  Size:   {_format_size(meta.original_size)}")


@main.command()
@click.argument("file", type=click.Path(exists=True))
@click.option("--repo", "-r", default=None, help="Repository name")
@click.option("--workspace", "-w", default=None, help="Workspace directory")
def append(file: str, repo: str | None, workspace: str | None):
    """Append a text file into an existing repository."""
    log_repo = open_repo(workspace, repo)

    before = log_repo.original_line_count()
    with console.status("Appending log file..."):
        added = log_repo.append_file(file)
    after = log_repo.original_line_count()

    console.print(f"[green]Appended:[/green] {file}")
    console.print(f"  New lines: {added:,}")
    console.print(f"  Total:     {before:,} -> {after:,}")


@main.command()
@click.option("--repo", "-r", default=None, help="Repository name")
@click.option("--workspace", "-w", default=None, help="Workspace directory")
def info(repo: str | None, workspace: str | None):
    """Show repository information."""
    ws = get_workspace(workspace)
    name = repo or ws.active()
    log_repo = ws.open_repo(name)
    meta = log_repo.metadata()
    history = log_repo.history()

    table = Table(title=f"Repository: {name}")
    table.add_column("Property", style="cyan")
    table.add_column("Value", style="green")

    table.add_row("ID", meta.id)
    table.add_row("Source", meta.source_name)
    table.add_row("Original Lines", f"{meta.original_line_count:,}")
    table.add_row("Original Size", _format_size(meta.original_size))
    table.add_row("Current Lines", f"{log_repo.current_line_count():,}")
    table.add_row("Operations", str(len(history)))
    table.add_row("Branches", str(len(log_repo.branch_names())))
    table.add_row("Current Branch", log_repo.current_branch_name())
    table.add_row("Created", meta.created_at)

    console.print(table)

    # Show branches
    branches = log_repo.branch_names()
    head_id = log_repo.head_node_id()
    if branches:
        branch_table = Table(title="Branches")
        branch_table.add_column("Name", style="green")
        branch_table.add_column("HEAD Node", style="cyan")
        branch_table.add_column("", style="dim")
        for b in branches:
            b_head = log_repo.branch_head_node_id(b)
            marker = "*" if b == log_repo.current_branch_name() else ""
            branch_table.add_row(
                f"{marker} {b}",
                str(b_head) if b_head is not None else "?",
                "(current)" if marker else ""
            )
        console.print()
        console.print(branch_table)


@main.command()
@click.option("--repo", "-r", default=None, help="Repository name")
@click.option("--workspace", "-w", default=None, help="Workspace directory")
@click.option("--start", "-s", default=0, help="Start line number (0-based)")
@click.option("--count", "-n", default=20, help="Number of lines to show")
@click.option("--numbers/--no-numbers", default=True, help="Show line numbers")
def view(repo: str | None, workspace: str | None, start: int, count: int, numbers: bool):
    """View lines from the current state of the log."""
    log_repo = open_repo(workspace, repo)
    lines = log_repo.read_lines(start, count)
    total = log_repo.current_line_count()

    if numbers:
        width = len(str(start + len(lines)))
        for i, line in enumerate(lines):
            line_num = start + i
            console.print(f"[dim]{line_num:>{width}}[/dim] {line}")
    else:
        for line in lines:
            console.print(line)

    console.print(f"\n[dim]Showing lines {start}-{start + len(lines) - 1} of {total:,}[/dim]")


@main.command()
@click.argument("pattern")
@click.option("--keep/--remove", default=True, help="Keep or remove matching lines")
@click.option("--repo", "-r", default=None, help="Repository name")
@click.option("--workspace", "-w", default=None, help="Workspace directory")
def filter(pattern: str, keep: bool, repo: str | None, workspace: str | None):
    """Filter lines by regex pattern.

    PATTERN is a regular expression. Use --keep (default) to keep matching
    lines, or --remove to remove them.
    """
    log_repo = open_repo(workspace, repo)

    before = log_repo.current_line_count()
    log_repo.filter(pattern, keep)
    after = log_repo.current_line_count()

    action = "kept" if keep else "removed"
    diff = abs(after - before)
    console.print(f"[green]Filter applied:[/green] {action} {diff:,} lines (/{pattern}/)")
    console.print(f"  Lines: {before:,} -> {after:,}")


@main.command()
@click.argument("pattern")
@click.argument("replacement")
@click.option("--repo", "-r", default=None, help="Repository name")
@click.option("--workspace", "-w", default=None, help="Workspace directory")
def replace(pattern: str, replacement: str, repo: str | None, workspace: str | None):
    """Replace text matching a regex pattern.

    PATTERN is a regular expression. REPLACEMENT can include capture
    group references like $1, $2, etc.
    """
    log_repo = open_repo(workspace, repo)
    log_repo.replace(pattern, replacement)
    console.print(f"[green]Replace applied:[/green] /{pattern}/ -> \"{replacement}\"")


@main.command()
@click.argument("indices", nargs=-1, type=int, required=True)
@click.option("--repo", "-r", default=None, help="Repository name")
@click.option("--workspace", "-w", default=None, help="Workspace directory")
def delete(indices: tuple[int, ...], repo: str | None, workspace: str | None):
    """Delete specific lines by their indices (0-based)."""
    log_repo = open_repo(workspace, repo)
    log_repo.delete_lines(list(indices))
    console.print(f"[green]Deleted {len(indices)} line(s)[/green]")


@main.command()
@click.argument("after_line", type=int)
@click.argument("content", nargs=-1, required=True)
@click.option("--repo", "-r", default=None, help="Repository name")
@click.option("--workspace", "-w", default=None, help="Workspace directory")
def insert(after_line: int, content: tuple[str, ...], repo: str | None, workspace: str | None):
    """Insert lines after the specified position.

    AFTER_LINE is the position to insert after (0 = insert at beginning).
    """
    log_repo = open_repo(workspace, repo)
    log_repo.insert_lines(after_line, list(content))
    console.print(f"[green]Inserted {len(content)} line(s) after line {after_line}[/green]")


@main.command()
@click.argument("line_index", type=int)
@click.argument("new_content")
@click.option("--repo", "-r", default=None, help="Repository name")
@click.option("--workspace", "-w", default=None, help="Workspace directory")
def modify(line_index: int, new_content: str, repo: str | None, workspace: str | None):
    """Modify a single line by its index (0-based)."""
    log_repo = open_repo(workspace, repo)
    log_repo.modify_line(line_index, new_content)
    console.print(f"[green]Modified line {line_index}[/green]")


@main.command()
@click.argument("sources", nargs=-1, required=True)
@click.option("--into", "-i", "target", required=True, help="Target repository name")
@click.option("--workspace", "-w", default=None, help="Workspace directory")
def merge(sources: tuple[str, ...], target: str, workspace: str | None):
    """Merge multiple repositories into a new one (in order).

    SOURCES are repo names whose current state (after all operations)
    will be concatenated in the given order into a new repo TARGET.

    Example:
      lograph-cli merge repoA repoB repoC --into combined
    """
    ws = get_workspace(workspace)
    if not ws.is_initialized():
        console.print("[red]No workspace found. Use 'import' to create one.[/red]")
        sys.exit(1)

    with console.status(f"Merging {', '.join(sources)} -> {target}..."):
        merged = ws.merge_repos(list(sources), target)

    meta = merged.metadata()
    console.print(f"[green]Merged {len(sources)} repo(s) into '{target}'[/green]")
    console.print(f"  Sources: {', '.join(sources)}")
    console.print(f"  Lines:   {meta.original_line_count:,}")


@main.command()
@click.option("--repo", "-r", default=None, help="Repository name")
@click.option("--workspace", "-w", default=None, help="Workspace directory")
def undo(repo: str | None, workspace: str | None):
    """Undo the last operation."""
    log_repo = open_repo(workspace, repo)
    desc = log_repo.undo()
    console.print(f"[green]Undone:[/green] {desc}")


@main.command()
@click.option("--repo", "-r", default=None, help="Repository name")
@click.option("--workspace", "-w", default=None, help="Workspace directory")
def history(repo: str | None, workspace: str | None):
    """Show operation history."""
    log_repo = open_repo(workspace, repo)

    records = log_repo.history()
    if not records:
        console.print("[dim]No operations applied yet.[/dim]")
        return

    table = Table(title="Operation History")
    table.add_column("ID", style="cyan")
    table.add_column("Operation", style="green")
    table.add_column("Applied At", style="dim")

    for record in records:
        table.add_row(str(record.id), record.description, record.applied_at)

    console.print(table)


@main.command()
@click.argument("dest", type=click.Path())
@click.option("--repo", "-r", default=None, help="Repository name")
@click.option("--workspace", "-w", default=None, help="Workspace directory")
def export(dest: str, repo: str | None, workspace: str | None):
    """Export current state to a text file."""
    log_repo = open_repo(workspace, repo)

    with console.status("Exporting..."):
        log_repo.export(dest)

    console.print(f"[green]Exported to:[/green] {dest}")


@main.command()
@click.argument("pattern")
@click.option("--repo", "-r", default=None, help="Repository name")
@click.option("--workspace", "-w", default=None, help="Workspace directory")
@click.option("--count", "-n", default=20, help="Max results to show")
def search(pattern: str, repo: str | None, workspace: str | None, count: int):
    """Search for lines matching a regex pattern (read-only, no modification)."""
    log_repo = open_repo(workspace, repo)

    results = log_repo.stream_search(pattern, count)
    if not results:
        console.print(f"[dim]No matches found for /{pattern}/[/dim]")
        return

    for line_num, content in results:
        console.print(f"[dim]{line_num:>8}[/dim] {content}")

    console.print(f"\n[green]{len(results)} match(es) shown[/green]")


@main.command(name="search-file")
@click.argument("file", type=click.Path(exists=True))
@click.argument("pattern")
@click.option("--count", "-n", default=50, help="Max results to show")
def search_file(file: str, pattern: str, count: int):
    """Search a file directly for lines matching a regex (no import needed).

    Uses ripgrep's SIMD-accelerated searcher on the raw file.
    """
    # Use the static method — doesn't need a repo
    results = LogRepo.search_file(file, pattern, count)
    if not results:
        console.print(f"[dim]No matches found for /{pattern}/[/dim]")
        return

    for line_num, content in results:
        console.print(f"[dim]{line_num:>8}[/dim] {content}")

    total_matches = LogRepo.count_file_matches(file, pattern)
    shown = len(results)
    if total_matches > shown:
        console.print(f"\n[green]{shown} of {total_matches} match(es) shown[/green]")
    else:
        console.print(f"\n[green]{total_matches} match(es)[/green]")


# ---------------------------------------------------------------------------
# stats subcommand group (analytics / collectors)
# ---------------------------------------------------------------------------

@main.group()
def stats():
    """Analytics and statistics on repositories."""
    pass


@stats.command(name="overview")
@click.option("--repo", "-r", default=None, help="Repository name")
@click.option("--workspace", "-w", default=None, help="Workspace directory")
def stats_overview(repo: str | None, workspace: str | None):
    """Show overview statistics for a repository."""
    log_repo = open_repo(workspace, repo)
    s = log_repo.stats()

    table = Table(title="Repository Statistics")
    table.add_column("Metric", style="cyan")
    table.add_column("Value", style="green", justify="right")

    table.add_row("Total Lines", f"{s.total_lines:,}")
    table.add_row("Total Bytes", _format_size(s.total_bytes))
    table.add_row("Avg Line Length", f"{s.avg_line_len:.1f}")
    table.add_row("Max Line Length", str(s.max_line_len))
    table.add_row("Min Line Length", str(s.min_line_len))
    table.add_row("Chunks", str(s.chunk_count))

    console.print(table)

    # Also show line stats from collector
    ls = log_repo.collect_original_line_stats()
    if ls:
        console.print()
        console.print(f"[dim]Original data: {ls['count']:,} lines, "
                      f"{_format_size(ls['total_bytes'])}, "
                      f"avg {ls['avg_len']:.1f} chars/line[/dim]")


@stats.command(name="count")
@click.argument("pattern", required=False)
@click.option("--repo", "-r", default=None, help="Repository name")
@click.option("--workspace", "-w", default=None, help="Workspace directory")
def stats_count(pattern: str | None, repo: str | None, workspace: str | None):
    """Count lines, optionally matching a regex pattern."""
    log_repo = open_repo(workspace, repo)
    n = log_repo.collect_original_count(pattern)
    if pattern:
        console.print(f"[green]{n:,}[/green] lines match /{pattern}/")
    else:
        console.print(f"[green]{n:,}[/green] total lines")


@stats.command(name="group-count")
@click.argument("pattern")
@click.option("--group", "-g", type=int, default=1, help="Capture group number (1-based)")
@click.option("--repo", "-r", default=None, help="Repository name")
@click.option("--workspace", "-w", default=None, help="Workspace directory")
def stats_group_count(pattern: str, group: int, repo: str | None, workspace: str | None):
    """Group lines by regex capture group and show counts."""
    log_repo = open_repo(workspace, repo)
    pairs = log_repo.collect_original_group_count(pattern, group)

    if not pairs:
        console.print("[dim]No matches found.[/dim]")
        return

    table = Table(title=f"Group Count: /{pattern}/")
    table.add_column("Value", style="green")
    table.add_column("Count", style="cyan", justify="right")

    for k, v in pairs.items():
        table.add_row(k, f"{v:,}")

    console.print(table)


@stats.command(name="top")
@click.argument("pattern")
@click.option("--group", "-g", type=int, default=1, help="Capture group number (1-based)")
@click.option("--limit", "-n", type=int, default=10, help="Number of top entries")
@click.option("--repo", "-r", default=None, help="Repository name")
@click.option("--workspace", "-w", default=None, help="Workspace directory")
def stats_top(pattern: str, group: int, limit: int, repo: str | None, workspace: str | None):
    """Top-N most frequent values of a regex capture group."""
    log_repo = open_repo(workspace, repo)
    pairs = log_repo.collect_original_top_n(pattern, group, limit)

    if not pairs:
        console.print("[dim]No matches found.[/dim]")
        return

    table = Table(title=f"Top {limit}: /{pattern}/")
    table.add_column("Value", style="green")
    table.add_column("Count", style="cyan", justify="right")

    for k, v in pairs:
        table.add_row(k, f"{v:,}")

    console.print(table)


@stats.command(name="distinct")
@click.argument("pattern")
@click.option("--group", "-g", type=int, default=1, help="Capture group number (1-based)")
@click.option("--repo", "-r", default=None, help="Repository name")
@click.option("--workspace", "-w", default=None, help="Workspace directory")
def stats_distinct(pattern: str, group: int, repo: str | None, workspace: str | None):
    """Show distinct values of a regex capture group."""
    log_repo = open_repo(workspace, repo)
    values = log_repo.collect_original_unique(pattern, group)

    if not values:
        console.print("[dim]No matches found.[/dim]")
        return

    console.print(f"[bold]Distinct values ({len(values)}):[/bold]")
    for val in values:
        console.print(f"  {val}")


@stats.command(name="numbers")
@click.argument("pattern")
@click.option("--group", "-g", type=int, default=1, help="Capture group number (1-based)")
@click.option("--repo", "-r", default=None, help="Repository name")
@click.option("--workspace", "-w", default=None, help="Workspace directory")
def stats_numbers(pattern: str, group: int, repo: str | None, workspace: str | None):
    """Compute numeric statistics (min/max/avg/sum) from a capture group."""
    log_repo = open_repo(workspace, repo)
    s = log_repo.collect_original_numeric_stats(pattern, group)

    if s['count'] == 0:
        console.print("[dim]No numeric values found.[/dim]")
        return

    table = Table(title=f"Numeric Stats: /{pattern}/")
    table.add_column("Metric", style="cyan")
    table.add_column("Value", style="green", justify="right")

    table.add_row("Count", f"{s['count']:,}")
    table.add_row("Sum", f"{s['sum']:,.2f}")
    table.add_row("Min", f"{s['min']:,.2f}")
    table.add_row("Max", f"{s['max']:,.2f}")
    table.add_row("Average", f"{s['avg']:,.2f}")

    console.print(table)


def _format_size(size_bytes: int) -> str:
    """Format byte size to human-readable string."""
    for unit in ["B", "KB", "MB", "GB", "TB"]:
        if size_bytes < 1024:
            return f"{size_bytes:.1f} {unit}"
        size_bytes /= 1024
    return f"{size_bytes:.1f} PB"


# ---------------------------------------------------------------------------
# node subcommand group — history node operations
# ---------------------------------------------------------------------------

@main.group()
def node():
    """Operate on history nodes (merge, subtract, delete)."""
    pass


@node.command(name="merge")
@click.argument("source_ids", nargs=-1, type=int, required=True)
@click.option("--mode", "-m", default="or",
              type=click.Choice(["or", "union", "and", "intersection", "sub", "subtract", "xor"]),
              help="Merge mode: or/union, and/intersection, sub/subtract, xor")
@click.option("--branch", "-b", default=None, help="Branch name for the new node")
@click.option("--repo", "-r", default=None, help="Repository name")
@click.option("--workspace", "-w", default=None, help="Workspace directory")
def node_merge(source_ids: tuple[int, ...], mode: str, branch: str | None, repo: str | None, workspace: str | None):
    """Merge multiple history nodes with a set operation mode."""
    log_repo = open_repo(workspace, repo)
    sources = list(source_ids)
    branch_name = branch or f"merge-{'-'.join(str(s) for s in sources)}"
    new_id = log_repo.merge_nodes(sources, branch_name, mode)
    console.print(f"[green]Merged {len(sources)} nodes ({mode}) → new node {new_id} on branch '{branch_name}'[/green]")


@node.command(name="subtract")
@click.argument("base_id", type=int)
@click.argument("subtrahend_id", type=int)
@click.option("--branch", "-b", default=None, help="Branch name for the new node")
@click.option("--repo", "-r", default=None, help="Repository name")
@click.option("--workspace", "-w", default=None, help="Workspace directory")
def node_subtract(base_id: int, subtrahend_id: int, branch: str | None, repo: str | None, workspace: str | None):
    """Subtract one node's results from another (set difference)."""
    log_repo = open_repo(workspace, repo)
    branch_name = branch or f"diff-{base_id}-{subtrahend_id}"
    new_id = log_repo.subtract_nodes(base_id, subtrahend_id, branch_name)
    console.print(f"[green]Subtracted node {subtrahend_id} from node {base_id} → new node {new_id}[/green]")


@node.command(name="delete")
@click.argument("node_id", type=int)
@click.option("--repo", "-r", default=None, help="Repository name")
@click.option("--workspace", "-w", default=None, help="Workspace directory")
def node_delete(node_id: int, repo: str | None, workspace: str | None):
    """Soft-delete a history node (pattern preserved in history)."""
    log_repo = open_repo(workspace, repo)
    log_repo.soft_delete_node(node_id)
    console.print(f"[green]Soft-deleted node {node_id}[/green]")


# ---------------------------------------------------------------------------
# tag subcommand group — tag management
# ---------------------------------------------------------------------------

@main.group()
def tag():
    """Manage line-range tags for scoped operations."""
    pass


@tag.command(name="list")
@click.option("--repo", "-r", default=None, help="Repository name")
@click.option("--workspace", "-w", default=None, help="Workspace directory")
def tag_list(repo: str | None, workspace: str | None):
    """List all tags for a repository."""
    ws_root = workspace or DEFAULT_WORKSPACE
    ts = TagStore(ws_root)
    name = repo or "default"
    tags = ts.get_tags(name)
    if not tags:
        console.print("[dim]No tags found.[/dim]")
        return
    table = Table(title=f"Tags for '{name}'")
    table.add_column("Name", style="green")
    table.add_column("Ranges", style="cyan")
    table.add_column("Created", style="dim")
    for t in tags:
        ranges_str = ", ".join(f"{s+1}-{e+1}" for s, e in t.ranges)
        table.add_row(t.name, ranges_str, t.created_at)
    console.print(table)


@tag.command(name="create")
@click.argument("name")
@click.option("--ranges", "-R", required=True, help="Line ranges, e.g. '10-50,100-200' (1-based)")
@click.option("--repo", "-r", default=None, help="Repository name")
@click.option("--workspace", "-w", default=None, help="Workspace directory")
def tag_create(name: str, ranges: str, repo: str | None, workspace: str | None):
    """Create a new tag with named line ranges."""
    ws_root = workspace or DEFAULT_WORKSPACE
    ts = TagStore(ws_root)
    repo_name = repo or "default"

    # Parse ranges string like "10-50,100-200"
    parsed = []
    for part in ranges.split(","):
        part = part.strip()
        if "-" in part:
            s, e = part.split("-", 1)
            parsed.append((int(s.strip()) - 1, int(e.strip()) - 1))  # convert to 0-based
        else:
            n = int(part) - 1
            parsed.append((n, n))
    ts.add_tag(repo_name, name, parsed)
    console.print(f"[green]Tag '{name}' created with ranges: {ranges}[/green]")


@tag.command(name="delete")
@click.argument("name")
@click.option("--repo", "-r", default=None, help="Repository name")
@click.option("--workspace", "-w", default=None, help="Workspace directory")
def tag_delete(name: str, repo: str | None, workspace: str | None):
    """Delete a tag."""
    ws_root = workspace or DEFAULT_WORKSPACE
    ts = TagStore(ws_root)
    repo_name = repo or "default"
    if ts.remove_tag(repo_name, name):
        console.print(f"[green]Tag '{name}' deleted[/green]")
    else:
        console.print(f"[red]Tag '{name}' not found[/red]")


@tag.command(name="rename")
@click.argument("old_name")
@click.argument("new_name")
@click.option("--repo", "-r", default=None, help="Repository name")
@click.option("--workspace", "-w", default=None, help="Workspace directory")
def tag_rename(old_name: str, new_name: str, repo: str | None, workspace: str | None):
    """Rename a tag."""
    ws_root = workspace or DEFAULT_WORKSPACE
    ts = TagStore(ws_root)
    repo_name = repo or "default"
    if ts.rename_tag(repo_name, old_name, new_name):
        console.print(f"[green]Tag '{old_name}' → '{new_name}'[/green]")
    else:
        console.print(f"[red]Tag '{old_name}' not found[/red]")


if __name__ == "__main__":
    main()
