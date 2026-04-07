"""Tests for Rust/Python benchmark reporting helpers."""

from __future__ import annotations

from tools.benchmark_jar_pipeline import summarize_samples
from tools.compare_rust_python_benchmarks import compare_reports, stage_speedup


def test_summarize_samples_reports_median_and_spread_for_odd_samples() -> None:
    assert summarize_samples([7, 3, 5]) == (5, 4, 3, 7)


def test_summarize_samples_reports_median_and_spread_for_even_samples() -> None:
    assert summarize_samples([9, 1, 5, 7]) == (6, 8, 1, 9)


def test_stage_speedup_returns_none_when_rust_median_is_zero() -> None:
    assert stage_speedup(0, 5) is None


def test_compare_reports_lines_up_stage_payloads() -> None:
    rust_payload = {
        "jar": "fixtures/example.jar",
        "iterations": 3,
        "stage_reports": [
            {
                "stage": "model-lift",
                "iterations": 3,
                "samples_milliseconds": [4, 5, 6],
                "median_milliseconds": 5,
                "spread_milliseconds": 2,
                "min_milliseconds": 4,
                "max_milliseconds": 6,
                "units": 2,
                "bytes": 10,
            }
        ],
    }
    python_payload = {
        "jar_path": "fixtures/example.jar",
        "stage_reports": [
            {
                "name": "model-lift",
                "iterations": 3,
                "samples_milliseconds": [10, 12, 14],
                "median_milliseconds": 12,
                "spread_milliseconds": 4,
                "min_milliseconds": 10,
                "max_milliseconds": 14,
            }
        ],
    }

    report = compare_reports(rust_payload, python_payload)

    assert report["jar"] == "fixtures/example.jar"
    assert report["iterations"] == 3
    assert report["stages"][0]["stage"] == "model-lift"
    assert report["stages"][0]["rust_speedup_vs_python"] == 2.4
