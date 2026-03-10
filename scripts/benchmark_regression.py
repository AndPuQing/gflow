#!/usr/bin/env python3
"""Run and summarize the benchmark regression suite for issue #129."""

from __future__ import annotations

import argparse
import json
import platform
import re
import shutil
import subprocess
import sys
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parent.parent
CRITERION_DIR = REPO_ROOT / "target" / "criterion"


@dataclass(frozen=True)
class BenchmarkCase:
    benchmark_id: str
    description: str


REGRESSION_SUITE = [
    BenchmarkCase(
        "bottleneck/job_creation/jobs/10000",
        "Scheduler insertion throughput for a realistic 10k-job backlog.",
    ),
    BenchmarkCase(
        "query/by_state/index/50000",
        "Indexed queued-job lookups at a mid-sized queue depth.",
    ),
    BenchmarkCase(
        "dependency/validate_circular/jobs/25000",
        "Dependency graph validation for large dependency sets.",
    ),
    BenchmarkCase(
        "group_concurrency/scheduling/jobs/25000",
        "Prepare-to-run scheduling with active group concurrency limits.",
    ),
    BenchmarkCase(
        "scheduling_flow/complete/jobs/25000",
        "End-to-end scheduling preparation with dependencies and resources.",
    ),
    BenchmarkCase(
        "reservation/scheduling_with_reservations/jobs_reservations/25000j_25r",
        "Reservation-aware scheduling with realistic queue and reservation counts.",
    ),
]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run the curated Criterion benchmark regression suite."
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    subparsers.add_parser("list", help="List the curated benchmark cases.")

    for command in ("save-baseline", "compare"):
        command_parser = subparsers.add_parser(
            command,
            help=(
                "Save a named Criterion baseline."
                if command == "save-baseline"
                else "Compare against a named Criterion baseline."
            ),
        )
        command_parser.add_argument(
            "--name",
            required=True,
            help="Criterion baseline name, for example ubuntu-22.04-stable.",
        )
        command_parser.add_argument(
            "--summary-json",
            default="target/benchmark-regression-summary.json",
            help="Path to write the machine-readable summary JSON.",
        )
        command_parser.add_argument(
            "--summary-markdown",
            default="target/benchmark-regression-summary.md",
            help="Path to write the Markdown summary.",
        )
        command_parser.add_argument(
            "--fail-threshold-pct",
            type=float,
            default=None,
            help=(
                "Fail when any benchmark regresses by more than this percentage. "
                "Only applies to compare."
            ),
        )
        command_parser.add_argument(
            "--clean",
            action="store_true",
            help="Remove target/criterion before running the suite.",
        )

    return parser.parse_args()


def run_command(command: list[str], *, cwd: Path | None = None) -> str:
    result = subprocess.run(
        command,
        cwd=cwd or REPO_ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        sys.stderr.write(result.stdout)
        sys.stderr.write(result.stderr)
        raise SystemExit(result.returncode)
    return result.stdout


def clean_criterion_dir() -> None:
    if CRITERION_DIR.exists():
        shutil.rmtree(CRITERION_DIR)


def list_suite() -> int:
    for index, case in enumerate(REGRESSION_SUITE, start=1):
        print(f"{index}. {case.benchmark_id}")
        print(f"   {case.description}")
    return 0


def run_suite(mode: str, baseline_name: str) -> list[dict[str, Any]]:
    results: list[dict[str, Any]] = []
    for case in REGRESSION_SUITE:
        criterion_args = [
            "cargo",
            "bench",
            "--bench",
            "scheduler_bench",
            "--",
            f"^{re.escape(case.benchmark_id)}$",
            "--noplot",
        ]
        if mode == "save-baseline":
            criterion_args.extend(["--save-baseline", baseline_name])
        else:
            criterion_args.extend(["--baseline", baseline_name])

        print(f"==> Running {case.benchmark_id}", flush=True)
        subprocess.run(criterion_args, cwd=REPO_ROOT, check=True)
        results.append(load_case_result(case, baseline_name))

    return results


def load_case_result(case: BenchmarkCase, baseline_name: str) -> dict[str, Any]:
    benchmark_file = find_benchmark_file(case.benchmark_id, "new")
    benchmark_data = load_json(benchmark_file)
    new_estimates = load_json(benchmark_file.with_name("estimates.json"))

    baseline_estimates = None
    baseline_file = find_benchmark_file(case.benchmark_id, baseline_name, required=False)
    if baseline_file is not None:
        baseline_estimates = load_json(baseline_file.with_name("estimates.json"))

    new_mean_ns = new_estimates["mean"]["point_estimate"]
    baseline_mean_ns = (
        baseline_estimates["mean"]["point_estimate"] if baseline_estimates else None
    )
    change_pct = None
    if baseline_mean_ns not in (None, 0):
        change_pct = ((new_mean_ns - baseline_mean_ns) / baseline_mean_ns) * 100.0

    return {
        "benchmark_id": case.benchmark_id,
        "directory_name": benchmark_data["directory_name"],
        "description": case.description,
        "new_mean_ns": new_mean_ns,
        "new_ci_low_ns": new_estimates["mean"]["confidence_interval"]["lower_bound"],
        "new_ci_high_ns": new_estimates["mean"]["confidence_interval"]["upper_bound"],
        "baseline_mean_ns": baseline_mean_ns,
        "change_pct": change_pct,
    }


def find_benchmark_file(
    benchmark_id: str, dataset_name: str, *, required: bool = True
) -> Path | None:
    if not CRITERION_DIR.exists():
        if required:
            raise FileNotFoundError(
                f"Criterion output directory {CRITERION_DIR} does not exist."
            )
        return None

    for candidate in CRITERION_DIR.rglob("benchmark.json"):
        if candidate.parent.name != dataset_name:
            continue
        data = load_json(candidate)
        if data.get("full_id") == benchmark_id:
            return candidate

    if required:
        raise FileNotFoundError(
            f"Could not find benchmark output for {benchmark_id!r} in dataset "
            f"{dataset_name!r} under {CRITERION_DIR}."
        )
    return None


def load_json(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text())


def collect_environment() -> dict[str, str]:
    rustc = run_command(["rustc", "-Vv"]).strip()
    cargo = run_command(["cargo", "-V"]).strip()
    return {
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "hostname": platform.node() or "unknown",
        "platform": platform.platform(),
        "python": platform.python_version(),
        "cargo": cargo,
        "rustc": rustc,
    }


def write_summary(
    *,
    mode: str,
    baseline_name: str,
    results: list[dict[str, Any]],
    summary_json: Path,
    summary_markdown: Path,
) -> None:
    payload = {
        "mode": mode,
        "baseline_name": baseline_name,
        "suite": [case.benchmark_id for case in REGRESSION_SUITE],
        "environment": collect_environment(),
        "results": results,
    }
    summary_json.parent.mkdir(parents=True, exist_ok=True)
    summary_json.write_text(json.dumps(payload, indent=2) + "\n")

    lines = [
        "# Benchmark Regression Summary",
        "",
        f"- Mode: `{mode}`",
        f"- Baseline: `{baseline_name}`",
        f"- Generated: `{payload['environment']['generated_at']}`",
        f"- Platform: `{payload['environment']['platform']}`",
        f"- Cargo: `{payload['environment']['cargo']}`",
        "",
        "| Benchmark | New mean | Baseline mean | Delta |",
        "| --- | ---: | ---: | ---: |",
    ]

    for result in results:
        new_mean = format_ns(result["new_mean_ns"])
        baseline_mean = (
            format_ns(result["baseline_mean_ns"])
            if result["baseline_mean_ns"] is not None
            else "n/a"
        )
        delta = (
            format_pct(result["change_pct"]) if result["change_pct"] is not None else "n/a"
        )
        lines.append(
            f"| `{result['benchmark_id']}` | {new_mean} | {baseline_mean} | {delta} |"
        )

    lines.extend(
        [
            "",
            "Representative suite:",
        ]
    )
    for case in REGRESSION_SUITE:
        lines.append(f"- `{case.benchmark_id}`: {case.description}")

    summary_markdown.parent.mkdir(parents=True, exist_ok=True)
    summary_markdown.write_text("\n".join(lines) + "\n")


def format_ns(value: float | None) -> str:
    if value is None:
        return "n/a"
    if value >= 1_000_000_000:
        return f"{value / 1_000_000_000:.2f} s"
    if value >= 1_000_000:
        return f"{value / 1_000_000:.2f} ms"
    if value >= 1_000:
        return f"{value / 1_000:.2f} us"
    return f"{value:.2f} ns"


def format_pct(value: float) -> str:
    return f"{value:+.2f}%"


def enforce_threshold(results: list[dict[str, Any]], threshold_pct: float | None) -> None:
    if threshold_pct is None:
        return

    regressions = [
        result
        for result in results
        if result["change_pct"] is not None and result["change_pct"] > threshold_pct
    ]
    if not regressions:
        return

    sys.stderr.write(
        "Benchmarks regressed beyond threshold "
        f"({threshold_pct:.2f}%):\n"
    )
    for result in regressions:
        sys.stderr.write(
            f"  - {result['benchmark_id']}: {format_pct(result['change_pct'])}\n"
        )
    raise SystemExit(1)


def main() -> int:
    args = parse_args()
    if args.command == "list":
        return list_suite()

    if args.clean:
        clean_criterion_dir()

    results = run_suite(args.command, args.name)
    write_summary(
        mode=args.command,
        baseline_name=args.name,
        results=results,
        summary_json=REPO_ROOT / args.summary_json,
        summary_markdown=REPO_ROOT / args.summary_markdown,
    )
    if args.command == "compare":
        enforce_threshold(results, args.fail_threshold_pct)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
