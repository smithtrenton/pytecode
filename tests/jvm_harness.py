"""JVM verification and execution harness for pytecode validation testing.

Wraps the ``VerifierHarness.java`` classloader harness with typed Python
entry points and result dataclasses.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

from tests.helpers import run_verifier_harness


@dataclass
class VerifyResult:
    """Outcome of a JVM verification attempt."""

    status: str  # "VERIFY_OK", "VERIFY_FAIL", or "FORMAT_FAIL"
    message: str | None = None
    stdout: str | None = None
    exec_error: str | None = None

    @property
    def ok(self) -> bool:
        return self.status == "VERIFY_OK"


@dataclass
class ExecutionResult:
    """Outcome of loading and executing a class via the JVM."""

    status: str
    stdout: str | None = None
    message: str | None = None
    exec_error: str | None = None

    @property
    def ok(self) -> bool:
        return self.status == "VERIFY_OK" and self.exec_error is None


def verify_class(class_path: Path, *, extra_classpath: list[Path] | None = None) -> VerifyResult:
    """Verify a ``.class`` file using the JVM with ``-Xverify:all``."""
    raw = run_verifier_harness(class_path, extra_classpath=extra_classpath)
    return VerifyResult(
        status=raw.get("status", "UNKNOWN"),
        message=raw.get("message"),
    )


def execute_class(
    class_path: Path,
    class_name: str,
    args: list[str] | None = None,
    *,
    extra_classpath: list[Path] | None = None,
) -> ExecutionResult:
    """Load, verify, and execute a class via the JVM harness."""
    raw = run_verifier_harness(
        class_path,
        execute=True,
        class_name=class_name,
        args=args,
        extra_classpath=extra_classpath,
    )
    return ExecutionResult(
        status=raw.get("status", "UNKNOWN"),
        stdout=raw.get("stdout"),
        message=raw.get("message"),
        exec_error=raw.get("exec_error"),
    )
