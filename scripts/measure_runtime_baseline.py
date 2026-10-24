"""Repeat the existing isolated 1,000-Post Service search observation.

This launches only a previously built Rust search test executable. It does not
invoke Cargo, rebuild, launch the GUI/runtime service, or use production data.
The test itself creates and removes a temporary Service root on every run.
"""

import argparse
import ctypes
import datetime
import hashlib
import json
import math
import os
import platform
import re
import subprocess
import sys
import time
from pathlib import Path


TEST_NAME = "bounded_search_observation_on_one_thousand_current_posts"
OBSERVATION = re.compile(
    r"search observation: current_posts=(\d+) returned_items=(\d+) service_elapsed_ms=(\d+)"
)


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def total_physical_memory_bytes():
    if os.name != "nt":
        return None

    class MemoryStatusEx(ctypes.Structure):
        _fields_ = [
            ("dwLength", ctypes.c_ulong),
            ("dwMemoryLoad", ctypes.c_ulong),
            ("ullTotalPhys", ctypes.c_ulonglong),
            ("ullAvailPhys", ctypes.c_ulonglong),
            ("ullTotalPageFile", ctypes.c_ulonglong),
            ("ullAvailPageFile", ctypes.c_ulonglong),
            ("ullTotalVirtual", ctypes.c_ulonglong),
            ("ullAvailVirtual", ctypes.c_ulonglong),
            ("ullAvailExtendedVirtual", ctypes.c_ulonglong),
        ]

    status = MemoryStatusEx()
    status.dwLength = ctypes.sizeof(status)
    if not ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(status)):
        return None
    return status.ullTotalPhys


def choose_existing_runner(root):
    candidates = sorted((root / "target" / "debug" / "deps").glob("search-*.exe"))
    if not candidates:
        raise RuntimeError("no existing target/debug search test executable contains the observation")
    # Probe only the newest cached runner. Walking every stale hash can take
    # minutes and adds no evidence once the current candidate is identified.
    candidate = max(candidates, key=lambda path: path.stat().st_mtime_ns)
    try:
        listing = subprocess.run(
            [str(candidate), "--list"], cwd=root, capture_output=True,
            text=True, encoding="utf-8", errors="replace", timeout=10, check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise RuntimeError("newest cached search test runner could not be inspected within 10 seconds") from error
    if listing.returncode != 0 or f"{TEST_NAME}: test" not in listing.stdout:
        raise RuntimeError("newest cached search test executable does not contain the observation")
    return candidate


def run_once(root, runner):
    started = time.perf_counter()
    try:
        result = subprocess.run(
            [str(runner), TEST_NAME, "--exact", "--nocapture"],
            cwd=root, capture_output=True, text=True, encoding="utf-8",
            errors="replace", timeout=90, check=False,
        )
    except subprocess.TimeoutExpired as error:
        raise RuntimeError("existing isolated Service observation exceeded 90 seconds") from error
    wall_ms = round((time.perf_counter() - started) * 1000, 3)
    if result.returncode != 0:
        # Captured test output is intentionally not echoed; it could contain
        # fixture diagnostics that are not appropriate for the evidence report.
        raise RuntimeError(f"isolated Service observation exited with code {result.returncode}")
    found = OBSERVATION.search(result.stdout)
    if not found:
        raise RuntimeError("test succeeded but did not emit the expected bounded observation")
    posts, returned, elapsed = (int(value) for value in found.groups())
    if posts != 1000 or returned != 50:
        raise RuntimeError("test output did not match the expected 1,000-Post/50-result fixture")
    return {"current_posts": posts, "returned_items": returned,
            "service_search_elapsed_ms": elapsed, "test_process_wall_ms": wall_ms}


def percentile_nearest_rank(values, fraction):
    ordered = sorted(values)
    return ordered[max(0, math.ceil(fraction * len(ordered)) - 1)]


def build_report(root, run_count):
    runner = choose_existing_runner(root)
    runs = [run_once(root, runner) for _ in range(run_count)]
    search_times = [run["service_search_elapsed_ms"] for run in runs]
    return {
        "schema_version": 1,
        "recorded_at": datetime.datetime.now().astimezone().isoformat(),
        "release_approved": False,
        "formal_ac_026_pass": False,
        "measurement_scope": (
            "Existing isolated Rust Service test only: each test run creates a temporary root, "
            "inserts 1,000 current Posts, searches for a 50-result page, then drops the fixture. "
            "No runtime service, GUI, installer, or production root is launched or measured."
        ),
        "environment": {
            "os": platform.platform(),
            "os_version": platform.version(),
            "architecture": platform.machine(),
            "logical_cpu_count": os.cpu_count(),
            "total_physical_memory_bytes": total_physical_memory_bytes(),
            "python": platform.python_version(),
            "runner_configuration": "existing target/debug Rust test executable; no build performed",
            "runner_path": runner.relative_to(root).as_posix(),
            "runner_size_bytes": runner.stat().st_size,
            "runner_sha256": sha256(runner),
        },
        "workload": {
            "source_test": "crates/swarm-core/tests/search.rs::" + TEST_NAME,
            "fixture_posts": 1000,
            "query_scope": "post",
            "returned_page_size": 50,
            "repetitions": run_count,
            "isolation": "one newly created and automatically removed temporary Service root per repetition",
        },
        "runs": runs,
        "service_search_elapsed_ms": {
            "min": min(search_times),
            "median": __import__("statistics").median(search_times),
            "p95_nearest_rank": percentile_nearest_rank(search_times, 0.95),
            "max": max(search_times),
        },
        "not_measured": [
            "idle runtime WorkingSet and PrivateMemory",
            "runtime-status or identity-status latency",
            "GUI memory and GUI interaction latency",
            "release-build, packaged, native Tauri, or user-data performance",
            "background system load and storage-device identity",
        ],
        "proposed_pre_acceptance_target": {
            "status": "maintainer proposal only; not user-approved and not a performance guarantee",
            "workload": "50-result Service search over 1,000 current Posts on a separately fixed acceptance environment",
            "suggested_p95_service_search_ms": 250,
            "acceptance_conditions": "review target and environment before acceptance; collect repeated release-build measurements and full AC-026 dimensions",
        },
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--output", type=Path, required=True)
    options = parser.parse_args()
    if not 1 <= options.runs <= 10:
        parser.error("--runs must be between 1 and 10")
    root = Path(__file__).resolve().parent.parent
    report = build_report(root, options.runs)
    with options.output.open("x", encoding="utf-8", newline="\n") as output:
        json.dump(report, output, ensure_ascii=False, indent=2)
        output.write("\n")
    print(json.dumps({
        "output": str(options.output.resolve()),
        "runner_sha256": report["environment"]["runner_sha256"],
        "runs": len(report["runs"]),
        "search_median_ms": report["service_search_elapsed_ms"]["median"],
        "search_p95_nearest_rank_ms": report["service_search_elapsed_ms"]["p95_nearest_rank"],
        "formal_ac_026_pass": False,
    }))


if __name__ == "__main__":
    main()
