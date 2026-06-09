"""
lograph: Log Graph Analyzer — high-performance log analyzer for large text files.

Uses a Rust backend with compressed storage, Git-like history DAG,
multi-threaded processing, and a Python CLI frontend.
"""

from lograph._core import LogRepo, RepoMetadata, OperationRecord, Tag, TagScope, TagStore, Workspace

__all__ = ["LogRepo", "RepoMetadata", "OperationRecord", "Tag", "TagScope", "TagStore", "Workspace"]
__version__ = "0.0.1"
