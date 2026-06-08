"""CLI tests using Click's CliRunner.

-w/--workspace is a per-command option, so it must appear AFTER the
subcommand name (e.g. "import file.log -w /ws --repo test").
"""

import os
import tempfile

import pytest
from click.testing import CliRunner

from log_analyzer.cli import main


@pytest.fixture
def runner():
    return CliRunner()


@pytest.fixture
def sample_log():
    """Create a temporary log file with known content."""
    with tempfile.NamedTemporaryFile(mode="w", suffix=".log", delete=False) as f:
        for i in range(100):
            level = ["INFO", "WARN", "ERROR", "DEBUG"][i % 4]
            f.write(f"2024-01-{(i % 28) + 1:02d} {level} [thread-{i % 4}] message {i}\n")
        path = f.name
    yield path
    os.unlink(path)


@pytest.fixture
def ws():
    """Create a temporary workspace directory."""
    with tempfile.TemporaryDirectory() as d:
        yield d


@pytest.fixture
def ws_repo(ws, sample_log):
    """Create a workspace with an imported repo named 'test'."""
    runner = CliRunner()
    r = runner.invoke(main, ["import", sample_log, "-w", ws, "--repo", "test"])
    assert r.exit_code == 0, f"import failed: {r.output}"
    return ws


# ── Basic CLI tests ──


class TestImport:
    def test_import_file(self, runner, ws, sample_log):
        r = runner.invoke(main, ["import", sample_log, "-w", ws, "--repo", "mylog"])
        assert r.exit_code == 0
        assert "mylog" in r.output

    def test_import_file_not_found(self, runner, ws):
        r = runner.invoke(main, ["import", "/nonexistent/file.log", "-w", ws])
        assert r.exit_code != 0


class TestView:
    def test_view_default(self, runner, ws_repo):
        r = runner.invoke(main, ["view", "-w", ws_repo, "--repo", "test"])
        assert r.exit_code == 0
        assert "message" in r.output

    def test_view_with_range(self, runner, ws_repo):
        r = runner.invoke(
            main, ["view", "-w", ws_repo, "--repo", "test", "--start", "5", "--count", "3"]
        )
        assert r.exit_code == 0
        assert "message 5" in r.output
        assert "message 7" in r.output

    def test_view_no_numbers(self, runner, ws_repo):
        r = runner.invoke(main, ["view", "-w", ws_repo, "--repo", "test", "--no-numbers"])
        assert r.exit_code == 0


class TestInfo:
    def test_info(self, runner, ws_repo):
        r = runner.invoke(main, ["info", "-w", ws_repo, "--repo", "test"])
        assert r.exit_code == 0
        assert "test" in r.output
        assert "100" in r.output

    def test_info_defaults_to_active(self, runner, ws_repo):
        r = runner.invoke(main, ["info", "-w", ws_repo])
        assert r.exit_code == 0


class TestSearch:
    def test_search_matches(self, runner, ws_repo):
        r = runner.invoke(main, ["search", "ERROR", "-w", ws_repo, "--repo", "test"])
        assert r.exit_code == 0
        assert "ERROR" in r.output

    def test_search_no_matches(self, runner, ws_repo):
        r = runner.invoke(main, ["search", "ZZZ_NO_MATCH_ZZZ", "-w", ws_repo, "--repo", "test"])
        assert r.exit_code == 0
        assert "No matches found" in r.output

    def test_search_with_limit(self, runner, ws_repo):
        r = runner.invoke(
            main, ["search", "message", "-w", ws_repo, "--repo", "test", "-n", "5"]
        )
        assert r.exit_code == 0
        assert "5 match(es)" in r.output


class TestFilter:
    def test_filter_keep(self, runner, ws_repo):
        r = runner.invoke(main, ["filter", "ERROR", "-w", ws_repo, "--keep", "--repo", "test"])
        assert r.exit_code == 0
        assert "Filter applied" in r.output

    def test_filter_remove(self, runner, ws_repo):
        r = runner.invoke(main, ["filter", "DEBUG", "-w", ws_repo, "--remove", "--repo", "test"])
        assert r.exit_code == 0
        assert "Filter applied" in r.output


class TestUndo:
    def test_undo_after_filter(self, runner, ws_repo):
        runner.invoke(main, ["filter", "ERROR", "-w", ws_repo, "--keep", "--repo", "test"])
        r = runner.invoke(main, ["undo", "-w", ws_repo, "--repo", "test"])
        assert r.exit_code == 0
        assert "Undone" in r.output


class TestHistory:
    def test_history_empty(self, runner, ws_repo):
        r = runner.invoke(main, ["history", "-w", ws_repo, "--repo", "test"])
        assert r.exit_code == 0
        assert "No operations" in r.output

    def test_history_after_operation(self, runner, ws_repo):
        runner.invoke(main, ["filter", "ERROR", "-w", ws_repo, "--keep", "--repo", "test"])
        r = runner.invoke(main, ["history", "-w", ws_repo, "--repo", "test"])
        assert r.exit_code == 0
        assert "filter" in r.output.lower()


class TestExport:
    def test_export(self, runner, ws_repo):
        with tempfile.NamedTemporaryFile(suffix=".log", delete=False) as f:
            export_path = f.name
        try:
            r = runner.invoke(main, ["export", export_path, "-w", ws_repo, "--repo", "test"])
            assert r.exit_code == 0
            assert os.path.exists(export_path)
            with open(export_path) as f:
                content = f.read()
            assert len(content) > 0
        finally:
            os.unlink(export_path)


class TestReplace:
    def test_replace(self, runner, ws_repo):
        r = runner.invoke(
            main, ["replace", "ERROR", "CRITICAL", "-w", ws_repo, "--repo", "test"]
        )
        assert r.exit_code == 0
        assert "Replace applied" in r.output


class TestDelete:
    def test_delete_lines(self, runner, ws_repo):
        r = runner.invoke(main, ["delete", "0", "1", "2", "-w", ws_repo, "--repo", "test"])
        assert r.exit_code == 0
        assert "Deleted 3" in r.output


class TestInsert:
    def test_insert_lines(self, runner, ws_repo):
        r = runner.invoke(
            main,
            ["insert", "0", "new line 1", "new line 2", "-w", ws_repo, "--repo", "test"],
        )
        assert r.exit_code == 0
        assert "Inserted 2" in r.output


class TestModify:
    def test_modify_line(self, runner, ws_repo):
        r = runner.invoke(
            main, ["modify", "0", "Modified!", "-w", ws_repo, "--repo", "test"]
        )
        assert r.exit_code == 0
        assert "Modified line 0" in r.output


class TestAppend:
    def test_append_file(self, runner, ws_repo, sample_log):
        r = runner.invoke(main, ["append", sample_log, "-w", ws_repo, "--repo", "test"])
        assert r.exit_code == 0
        assert "Appended" in r.output


# ── Repo subcommand tests ──


class TestRepoList:
    def test_repo_list(self, runner, ws_repo):
        r = runner.invoke(main, ["repo", "list", "-w", ws_repo])
        assert r.exit_code == 0
        assert "test" in r.output

    def test_repo_list_empty(self, runner, ws):
        r = runner.invoke(main, ["repo", "list", "-w", ws])
        assert r.exit_code == 0


class TestRepoUse:
    def test_repo_use(self, runner, ws_repo):
        r = runner.invoke(main, ["repo", "use", "test", "-w", ws_repo])
        assert r.exit_code == 0
        assert "test" in r.output


class TestRepoClone:
    def test_repo_clone(self, runner, ws_repo):
        r = runner.invoke(main, ["repo", "clone", "test", "cloned", "-w", ws_repo])
        assert r.exit_code == 0
        lr = runner.invoke(main, ["repo", "list", "-w", ws_repo])
        assert "cloned" in lr.output


class TestRepoRemove:
    def test_repo_remove(self, runner, ws_repo):
        runner.invoke(main, ["repo", "clone", "test", "to_remove", "-w", ws_repo])
        r = runner.invoke(main, ["repo", "remove", "to_remove", "-w", ws_repo, "--yes"])
        assert r.exit_code == 0
        assert "Removed" in r.output


# ── Merge ──


class TestMerge:
    def test_merge_repos(self, runner, ws_repo):
        runner.invoke(main, ["repo", "clone", "test", "source2", "-w", ws_repo])
        r = runner.invoke(
            main, ["merge", "test", "source2", "-w", ws_repo, "--into", "merged"]
        )
        assert r.exit_code == 0
        assert "Merged" in r.output


# ── Error cases ──


class TestErrorCases:
    def test_missing_repo(self, runner, ws):
        r = runner.invoke(main, ["view", "-w", ws, "--repo", "nonexistent"])
        assert r.exit_code != 0

    def test_no_active_repo(self, runner, ws):
        r = runner.invoke(main, ["view", "-w", ws])
        assert r.exit_code != 0

    def test_unknown_command(self, runner, ws):
        r = runner.invoke(main, ["nonexistent_cmd", "-w", ws])
        assert r.exit_code != 0


# ── Branch subcommand tests ──


class TestBranchList:
    def test_branch_list(self, runner, ws_repo):
        r = runner.invoke(main, ["branch", "list", "-w", ws_repo, "--repo", "test"])
        assert r.exit_code == 0
        assert "main" in r.output

    def test_branch_list_no_repo(self, runner, ws):
        r = runner.invoke(main, ["branch", "list", "-w", ws, "--repo", "nonexistent"])
        assert r.exit_code != 0


class TestBranchCheckout:
    def test_branch_checkout_main(self, runner, ws_repo):
        r = runner.invoke(main, ["branch", "checkout", "main", "-w", ws_repo, "--repo", "test"])
        assert r.exit_code == 0
        assert "main" in r.output


class TestBranchCreate:
    def test_branch_create(self, runner, ws_repo):
        r = runner.invoke(main, ["branch", "create", "experiment", "-w", ws_repo, "--repo", "test"])
        assert r.exit_code == 0
        assert "experiment" in r.output

        # Verify it appears in list
        lr = runner.invoke(main, ["branch", "list", "-w", ws_repo, "--repo", "test"])
        assert "experiment" in lr.output

    def test_branch_create_duplicate_fails(self, runner, ws_repo):
        runner.invoke(main, ["branch", "create", "dup", "-w", ws_repo, "--repo", "test"])
        r = runner.invoke(main, ["branch", "create", "dup", "-w", ws_repo, "--repo", "test"])
        assert "already exists" in r.output


class TestBranchDelete:
    def test_branch_delete(self, runner, ws_repo):
        runner.invoke(main, ["branch", "create", "to_del", "-w", ws_repo, "--repo", "test"])
        r = runner.invoke(
            main, ["branch", "delete", "to_del", "-w", ws_repo, "--repo", "test", "--yes"]
        )
        assert r.exit_code == 0
        assert "deleted" in r.output

    def test_branch_delete_main_fails(self, runner, ws_repo):
        r = runner.invoke(
            main, ["branch", "delete", "main", "-w", ws_repo, "--repo", "test", "--yes"]
        )
        assert "Cannot delete 'main'" in r.output


# ── Stats subcommand tests ──


class TestStatsOverview:
    def test_stats_overview(self, runner, ws_repo):
        r = runner.invoke(main, ["stats", "overview", "-w", ws_repo, "--repo", "test"])
        assert r.exit_code == 0
        assert "Total Lines" in r.output


class TestStatsCount:
    def test_stats_count_all(self, runner, ws_repo):
        r = runner.invoke(main, ["stats", "count", "-w", ws_repo, "--repo", "test"])
        assert r.exit_code == 0
        assert "100" in r.output

    def test_stats_count_pattern(self, runner, ws_repo):
        r = runner.invoke(main, ["stats", "count", "ERROR", "-w", ws_repo, "--repo", "test"])
        assert r.exit_code == 0
        assert "25" in r.output


class TestStatsGroupCount:
    def test_stats_group_count(self, runner, ws_repo):
        r = runner.invoke(
            main,
            ["stats", "group-count", r"(INFO|WARN|ERROR|DEBUG)", "-w", ws_repo, "--repo", "test"],
        )
        assert r.exit_code == 0
        assert "INFO" in r.output
        assert "ERROR" in r.output


class TestStatsTop:
    def test_stats_top(self, runner, ws_repo):
        r = runner.invoke(
            main,
            ["stats", "top", r"(INFO|WARN|ERROR|DEBUG)", "-w", ws_repo, "--repo", "test", "-n", "4"],
        )
        assert r.exit_code == 0
        assert "25" in r.output


class TestStatsDistinct:
    def test_stats_distinct(self, runner, ws_repo):
        r = runner.invoke(
            main,
            ["stats", "distinct", r"(INFO|WARN|ERROR|DEBUG)", "-w", ws_repo, "--repo", "test"],
        )
        assert r.exit_code == 0
        assert "4" in r.output


class TestStatsNumbers:
    def test_stats_numbers(self, runner, ws_repo):
        r = runner.invoke(
            main,
            [
                "stats", "numbers",
                r"message (\d+)", "-g", "1",
                "-w", ws_repo, "--repo", "test",
            ],
        )
        assert r.exit_code == 0
        assert "Count" in r.output


# ── Search-file tests ──


class TestSearchFile:
    def test_search_file(self, runner, sample_log):
        r = runner.invoke(main, ["search-file", sample_log, "ERROR"])
        assert r.exit_code == 0
        assert "ERROR" in r.output
        assert "match" in r.output.lower()


# ── Enhanced info tests ──


class TestInfoWithBranches:
    def test_info_shows_branches(self, runner, ws_repo):
        r = runner.invoke(main, ["info", "-w", ws_repo, "--repo", "test"])
        assert r.exit_code == 0
        assert "main" in r.output
        assert "Current Branch" in r.output
        assert "Branches" in r.output
