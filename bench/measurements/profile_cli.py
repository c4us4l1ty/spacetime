#!/usr/bin/env python3
"""Exploratory Linux process profiler for Spacetime's existing headless CLI workloads.

This is deliberately not a resource-gate evaluator. It cannot measure the future native
workspace, cold boot, tab switching, snapshots, or physical-machine provenance.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import re
import signal
import statistics
import subprocess
import sys
import tempfile
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

SCRIPT_PATH = Path(__file__).resolve()
REPO_ROOT = SCRIPT_PATH.parents[2]
PROFILE_REGISTRY = REPO_ROOT / "bench" / "targets" / "profiles.tsv"
MAX_RUNS = 50
MAX_TIMEOUT_SECONDS = 1800
SAFE_LABEL = re.compile(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,63}\Z")

WORKLOADS = {
    "verify": "full offline CLI verification; headless, no GUI or cell",
    "econbench": "scripted offline EconBench seed; no live model or account",
    "console-export": "static console JSON export from the checked-in sample ledger",
}


class ProfileError(Exception):
    """A bounded profiler input or execution error."""


def load_profiles(path: Path = PROFILE_REGISTRY) -> dict[str, dict[str, Any]]:
    profiles: dict[str, dict[str, Any]] = {}
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except OSError as error:
        raise ProfileError(f"cannot read target profile registry: {error}") from error

    for line_number, line in enumerate(lines, start=1):
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        columns = line.split("\t")
        if len(columns) != 7:
            raise ProfileError(f"invalid profile registry row at line {line_number}")
        profile_id, label, memory, boot, tab_switch, cpu, role = columns
        if not profile_id or profile_id in profiles:
            raise ProfileError(f"empty or duplicate target profile at line {line_number}")
        try:
            profile = {
                "id": profile_id,
                "label": label,
                "memory_budget_kb": int(memory),
                "boot_budget_ms": int(boot),
                "tab_switch_budget_ms": int(tab_switch),
                "cpu_budget_percent": int(cpu),
                "role": role,
            }
        except ValueError as error:
            raise ProfileError(f"invalid numeric profile budget at line {line_number}") from error
        if min(
            profile["memory_budget_kb"],
            profile["boot_budget_ms"],
            profile["tab_switch_budget_ms"],
            profile["cpu_budget_percent"],
        ) <= 0:
            raise ProfileError(f"non-positive profile budget at line {line_number}")
        if role not in {"baseline_gate", "optional_profile"}:
            raise ProfileError(f"invalid profile role at line {line_number}")
        profiles[profile_id] = profile

    baseline_ids = [profile_id for profile_id, row in profiles.items() if row["role"] == "baseline_gate"]
    if baseline_ids != ["4gb_potato"]:
        raise ProfileError("registry must contain exactly one baseline_gate profile: 4gb_potato")
    if not profiles:
        raise ProfileError("target profile registry is empty")
    return profiles


def validate_linux_procfs() -> None:
    if platform.system() != "Linux" or not Path("/proc/self/stat").is_file():
        raise ProfileError("the exploratory process sampler requires Linux /proc")


def validate_host_label(value: str) -> str:
    if not SAFE_LABEL.fullmatch(value):
        raise ProfileError("host label must be 1-64 ASCII letters, digits, dot, underscore, or hyphen")
    return value


def read_proc_stat(path: Path) -> tuple[int, int, int, int, int] | None:
    """Return pgrp, utime+stime, starttime, RSS pages, and PID from Linux proc stat."""
    try:
        text = path.read_text(encoding="ascii")
        # comm is parenthesized and may itself contain spaces or closing parentheses.
        close_paren = text.rfind(")")
        if close_paren < 0:
            return None
        fields = text[close_paren + 2 :].split()
        if len(fields) < 22:
            return None
        pid = int(path.parent.name)
        pgrp = int(fields[2])
        cpu_ticks = int(fields[11]) + int(fields[12])
        start_ticks = int(fields[19])
        rss_pages = max(0, int(fields[21]))
        return pgrp, cpu_ticks, start_ticks, rss_pages, pid
    except (OSError, UnicodeError, ValueError):
        return None


def read_vm_hwm_kb(pid: int) -> int | None:
    try:
        for line in Path(f"/proc/{pid}/status").read_text(encoding="ascii").splitlines():
            if line.startswith("VmHWM:"):
                fields = line.split()
                if len(fields) >= 3 and fields[2] == "kB":
                    return int(fields[1])
    except (OSError, UnicodeError, ValueError, IndexError):
        return None
    return None


def process_group_snapshot(process_group_id: int, page_size: int) -> list[tuple[int, int, int, int]]:
    members: list[tuple[int, int, int, int]] = []
    try:
        proc_entries = os.scandir("/proc")
    except OSError as error:
        raise ProfileError(f"cannot scan /proc: {error}") from error
    with proc_entries:
        for entry in proc_entries:
            if not entry.name.isdigit():
                continue
            stat = read_proc_stat(Path("/proc") / entry.name / "stat")
            if stat is None:
                continue
            pgrp, cpu_ticks, start_ticks, rss_pages, pid = stat
            if pgrp == process_group_id:
                rss_kb = rss_pages * page_size // 1024
                members.append((pid, start_ticks, cpu_ticks, rss_kb))
    return members


def workload_argv(binary: Path, workload: str, output_dir: Path) -> list[str]:
    if workload == "verify":
        return [str(binary), "verify", "--out", str(output_dir / "verify.toml")]
    if workload == "econbench":
        return [
            str(binary),
            "bench",
            "run",
            "--arm",
            "A",
            "--model",
            "scripted-fixture",
            "--budget",
            "1000",
            "--out",
            str(output_dir / "econbench.json"),
        ]
    if workload == "console-export":
        ledger = REPO_ROOT / "bench" / "econbench" / "sample-ledger.jsonl"
        return [str(binary), "console", "--ledger", str(ledger), "--out", str(output_dir / "console.json")]
    raise ProfileError(f"unknown workload: {workload}")


def run_once(
    binary: Path,
    workload: str,
    run_index: int,
    sampling_interval_ms: int,
    timeout_seconds: int,
) -> dict[str, Any]:
    page_size = os.sysconf("SC_PAGE_SIZE")
    clock_ticks_per_second = os.sysconf("SC_CLK_TCK")
    if page_size <= 0 or clock_ticks_per_second <= 0:
        raise ProfileError("Linux did not report a valid page size or CPU tick rate")

    with tempfile.TemporaryDirectory(prefix="spacetime-cli-profile-") as output_dir_text:
        output_dir = Path(output_dir_text)
        argv = workload_argv(binary, workload, output_dir)
        started_ns = time.monotonic_ns()
        deadline_ns = started_ns + timeout_seconds * 1_000_000_000
        try:
            process = subprocess.Popen(
                argv,
                cwd=REPO_ROOT,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                start_new_session=True,
                close_fds=True,
            )
        except OSError as error:
            raise ProfileError(f"cannot start CLI workload: {error}") from error

        process_group_id = process.pid
        sample_interval = sampling_interval_ms / 1000.0
        cpu_seen: dict[tuple[int, int], int] = {}
        cpu_ticks_observed = 0
        peak_group_rss_kb = 0
        root_vm_hwm_kb = 0
        peak_process_count = 0
        sample_count = 0
        timed_out = False
        return_code: int | None = None

        while True:
            members = process_group_snapshot(process_group_id, page_size)
            sample_count += 1
            sample_rss_kb = 0
            for pid, start_ticks, cpu_ticks, rss_kb in members:
                key = (pid, start_ticks)
                previous_ticks = cpu_seen.get(key)
                if previous_ticks is None:
                    cpu_ticks_observed += cpu_ticks
                elif cpu_ticks >= previous_ticks:
                    cpu_ticks_observed += cpu_ticks - previous_ticks
                cpu_seen[key] = cpu_ticks
                sample_rss_kb += rss_kb
                if pid == process.pid:
                    root_high_water = read_vm_hwm_kb(pid)
                    if root_high_water is not None:
                        root_vm_hwm_kb = max(root_vm_hwm_kb, root_high_water)

            peak_group_rss_kb = max(peak_group_rss_kb, sample_rss_kb)
            peak_process_count = max(peak_process_count, len(members))
            return_code = process.poll()
            now_ns = time.monotonic_ns()
            if return_code is not None and not members:
                break
            if now_ns >= deadline_ns:
                timed_out = True
                try:
                    os.killpg(process_group_id, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                try:
                    return_code = process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    return_code = None
                break
            time.sleep(sample_interval)

        if return_code is None:
            try:
                return_code = process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                return_code = -signal.SIGKILL

        finished_ns = time.monotonic_ns()
        wall_ms = max(0, (finished_ns - started_ns) // 1_000_000)
        cpu_time_ms = cpu_ticks_observed * 1000 // clock_ticks_per_second
        logical_cpus = max(1, os.cpu_count() or 1)
        cpu_one_core_percent = round(cpu_time_ms * 100 / wall_ms, 2) if wall_ms else 0.0
        cpu_host_percent = round(cpu_one_core_percent / logical_cpus, 2)

        return {
            "run": run_index,
            "exit_code": return_code,
            "timed_out": timed_out,
            "wall_ms": wall_ms,
            "sampled_process_group_peak_rss_kb": peak_group_rss_kb,
            "sampled_root_vm_hwm_kb": root_vm_hwm_kb or None,
            "sampled_cpu_time_ms": cpu_time_ms,
            "sampled_cpu_percent_of_one_core": cpu_one_core_percent,
            "sampled_cpu_percent_of_host": cpu_host_percent,
            "logical_cpus": logical_cpus,
            "peak_process_count": peak_process_count,
            "sample_count": sample_count,
        }


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def git_source_facts() -> dict[str, Any]:
    try:
        revision = subprocess.run(
            ["git", "-C", str(REPO_ROOT), "rev-parse", "--verify", "HEAD"],
            check=True,
            capture_output=True,
            text=True,
            timeout=5,
        ).stdout.strip()
        dirty = bool(
            subprocess.run(
                ["git", "-C", str(REPO_ROOT), "status", "--porcelain"],
                check=True,
                capture_output=True,
                text=True,
                timeout=5,
            ).stdout.strip()
        )
        return {"revision": revision, "working_tree_dirty": dirty}
    except (OSError, subprocess.SubprocessError):
        return {"revision": "unavailable", "working_tree_dirty": "unknown"}


def linux_host_facts() -> dict[str, Any]:
    memory_total_kb: int | None = None
    cpu_model: str | None = None
    try:
        for line in Path("/proc/meminfo").read_text(encoding="ascii").splitlines():
            if line.startswith("MemTotal:"):
                memory_total_kb = int(line.split()[1])
                break
    except (OSError, UnicodeError, ValueError, IndexError):
        pass
    try:
        for line in Path("/proc/cpuinfo").read_text(encoding="utf-8", errors="replace").splitlines():
            if line.lower().startswith(("model name", "hardware", "processor")) and ":" in line:
                cpu_model = line.split(":", 1)[1].strip()
                if cpu_model:
                    break
    except OSError:
        pass
    return {
        "os": platform.system(),
        "kernel": platform.release(),
        "architecture": platform.machine(),
        "cpu_model": cpu_model,
        "logical_cpus": os.cpu_count(),
        "proc_mem_total_kb": memory_total_kb,
        "memory_source": "/proc/meminfo; may not reflect container or cgroup limits",
        "hostname_recorded": False,
    }


def output_path(path_text: str) -> Path:
    path = Path(path_text).expanduser().resolve()
    try:
        path.relative_to(REPO_ROOT)
    except ValueError:
        pass
    else:
        raise ProfileError("profile output must be outside the repository; keep raw evidence out of Git")
    if path.exists():
        raise ProfileError("profile output already exists; choose a new path to preserve prior evidence")
    return path


def write_json_new(path: Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary_path: Path | None = None
    try:
        with tempfile.NamedTemporaryFile(
            mode="w",
            encoding="utf-8",
            dir=path.parent,
            prefix=f".{path.name}.",
            suffix=".tmp",
            delete=False,
        ) as output:
            temporary_path = Path(output.name)
            json.dump(value, output, ensure_ascii=False, indent=2, sort_keys=True)
            output.write("\n")
        # A check-then-replace sequence could overwrite another report created between
        # output_path() and this write. A hard link is an atomic create-if-absent operation
        # because the temporary file lives in the same directory/filesystem.
        os.link(temporary_path, path)
    finally:
        if temporary_path is not None and temporary_path.exists():
            temporary_path.unlink()


def make_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=(
            "Profile one existing headless spacetime-ctl workload on Linux. "
            "This is exploratory only and never creates a resource-gate measurement."
        )
    )
    parser.add_argument("--binary", required=True, help="path to a built spacetime-ctl executable")
    parser.add_argument("--profile", required=True, help="explicit ID from bench/targets/profiles.tsv")
    parser.add_argument("--host-label", required=True, help="safe caller-supplied label; not physical attestation")
    parser.add_argument("--workload", choices=sorted(WORKLOADS), required=True)
    parser.add_argument("--runs", type=int, default=5, help="number of repeated CLI runs (1..50; default 5)")
    parser.add_argument(
        "--sampling-ms",
        type=int,
        default=50,
        help="/proc process-group sampling interval in milliseconds (5..250; default 50)",
    )
    parser.add_argument(
        "--timeout-seconds",
        type=int,
        default=300,
        help="per-run process-group timeout (1..1800; default 300)",
    )
    parser.add_argument("--out", required=True, help="new JSON report path outside the repository")
    return parser


def build_report(args: argparse.Namespace) -> dict[str, Any]:
    validate_linux_procfs()
    profiles = load_profiles()
    if args.profile not in profiles:
        raise ProfileError(f"unregistered target profile: {args.profile}")
    profile = profiles[args.profile]
    host_label = validate_host_label(args.host_label)
    if not 1 <= args.runs <= MAX_RUNS:
        raise ProfileError(f"runs must be 1..={MAX_RUNS}")
    if not 5 <= args.sampling_ms <= 250:
        raise ProfileError("sampling interval must be 5..=250 ms")
    if not 1 <= args.timeout_seconds <= MAX_TIMEOUT_SECONDS:
        raise ProfileError(f"timeout must be 1..={MAX_TIMEOUT_SECONDS} seconds")
    if args.workload not in WORKLOADS:
        raise ProfileError(f"unknown workload: {args.workload}")

    binary = Path(args.binary).expanduser().resolve()
    if not binary.is_file() or not os.access(binary, os.X_OK):
        raise ProfileError("--binary must name an existing executable file")
    destination = output_path(args.out)
    binary_digest = sha256_file(binary)
    runs: list[dict[str, Any]] = []
    for index in range(1, args.runs + 1):
        run = run_once(binary, args.workload, index, args.sampling_ms, args.timeout_seconds)
        runs.append(run)
        if run["exit_code"] != 0 or run["timed_out"]:
            break

    succeeded = len(runs) == args.runs and all(
        run["exit_code"] == 0 and not run["timed_out"] for run in runs
    )
    wall_times = [run["wall_ms"] for run in runs]
    peak_rss = [run["sampled_process_group_peak_rss_kb"] for run in runs]
    report = {
        "schema_version": 1,
        "record_type": "exploratory_headless_cli_profile",
        "captured_utc": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "host_label": host_label,
        "source": {
            **git_source_facts(),
            "binary_sha256": binary_digest,
        },
        "selected_target_profile": profile,
        "host_facts": linux_host_facts(),
        "workload": {
            "id": args.workload,
            "description": WORKLOADS[args.workload],
            "command_shell_used": False,
            "network_isolation_established": False,
            "surface_or_cell_launched": False,
        },
        "instrumentation": {
            "platform": "Linux /proc",
            "sampling_interval_ms": args.sampling_ms,
            "scope": "sampled process group for one spacetime-ctl CLI command",
            "rss_metric": (
                "sampled sum of per-process RSS; shared pages may be counted more than once and "
                "short-lived peaks may be missed"
            ),
            "cpu_metric": "sampled process utime+stime ticks; brief children may be missed",
            "sampler_cpu_included": False,
            "observer_effect": "Python /proc sampling runs concurrently and may affect host load and wall time",
            "first_run_may_differ": True,
        },
        "runs": runs,
        "summary": {
            "cli_runs_succeeded": sum(
                run["exit_code"] == 0 and not run["timed_out"] for run in runs
            ),
            "cli_runs_requested": args.runs,
            "median_wall_ms": statistics.median(wall_times) if wall_times else None,
            "maximum_sampled_process_group_rss_kb": max(peak_rss) if peak_rss else None,
            "all_cli_runs_succeeded": succeeded,
        },
        "release_gate": {
            "resource_gate": "unmeasured_target_4gb",
            "performance_claims": "unmeasured",
            "block3_done": False,
            "gate_eligible": False,
            "missing_product_metrics": [
                "whole-workspace peak RSS under agreed concurrency",
                "cold product startup",
                "tab-switch and fresh-snapshot latency",
                "snapshot capture CPU and memory",
                "accepted out-of-band physical provenance",
            ],
            "reason": (
                "This records only existing headless CLI process-group samples. It does not run the "
                "native workspace, GUI surfaces, cells, or the target-machine product workload, "
                "and it cannot establish physical provenance."
            ),
        },
    }
    report["output_path"] = str(destination)
    write_json_new(destination, report)
    return report


def main(argv: list[str] | None = None) -> int:
    parser = make_parser()
    args = parser.parse_args(argv)
    try:
        report = build_report(args)
    except ProfileError as error:
        print(f"profile error: {error}", file=sys.stderr)
        return 2
    except OSError as error:
        print(f"profile error: {error}", file=sys.stderr)
        return 2

    summary = report["summary"]
    print(
        "profile_report={} workload={} cli_runs={}/{} resource_gate=unmeasured_target_4gb".format(
            report["output_path"],
            args.workload,
            summary["cli_runs_succeeded"],
            summary["cli_runs_requested"],
        )
    )
    return 0 if summary["all_cli_runs_succeeded"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
