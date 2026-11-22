"""Frame recomputation with incomplete hierarchies, and BootstrapMethods editing.

Each scenario compiles real Java, rewrites it, and runs it on the JVM with
``-Xverify:all``, so the recomputed frames are checked by HotSpot itself.
"""

from __future__ import annotations

import subprocess
import warnings
import zipfile
from pathlib import Path
from typing import Any

import pytest
from helpers import _jdk_tool, compile_java_sources

from pytecode import JarFile
from pytecode.analysis import MappingClassResolver
from pytecode.archive import FrameComputationMode, FrameComputationWarning
from pytecode.model import BootstrapArgument, BootstrapMethod, ClassModel, MethodHandleValue

JOINS_SOURCE = """
class Base { int value() { return 40; } }
class Left extends Base { int value() { return 41; } }
class Right extends Base { int value() { return 42; } }

public class Joins {
    static int pick(boolean flag, Left left, Right right) {
        Base chosen = flag ? left : right;
        return chosen.value();
    }

    public static void main(String[] args) {
        System.out.println(pick(true, new Left(), new Right()) + pick(false, new Left(), new Right()));
    }
}
"""

LAMBDA_SOURCE = """
public class Lambdas {
    public static void main(String[] args) {
        Runnable task = () -> System.out.println("lambda-ran");
        task.run();
    }
}
"""


def _compile(tmp_path: Path, name: str, source: str) -> Path:
    source_dir = tmp_path / "src"
    source_dir.mkdir(exist_ok=True)
    path = source_dir / f"{name}.java"
    path.write_text(source, encoding="utf-8")
    return compile_java_sources(tmp_path, [path], release=17)


def _run(classes_dir: Path, main: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [_jdk_tool("java"), "-Xverify:all", "-cp", str(classes_dir), main],
        capture_output=True,
        text=True,
        check=False,
    )


def _recompute(path: Path, resolver: MappingClassResolver | None = None) -> list[warnings.WarningMessage]:
    model = ClassModel.from_bytes(path.read_bytes())
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        path.write_bytes(model.to_bytes_with_options(frame_mode=FrameComputationMode.RECOMPUTE, resolver=resolver))
    return [warning for warning in caught if issubclass(warning.category, FrameComputationWarning)]


def test_recompute_without_resolver_infers_join_types(tmp_path: Path):
    classes = _compile(tmp_path, "Joins", JOINS_SOURCE)
    assert _recompute(classes / "Joins.class") == []
    result = _run(classes, "Joins")
    assert result.returncode == 0, result.stderr
    assert result.stdout.strip() == "83"


def test_recompute_with_partial_resolver_does_not_abort(tmp_path: Path):
    classes = _compile(tmp_path, "Joins", JOINS_SOURCE)
    joins = classes / "Joins.class"
    # The resolver knows the class being lowered but none of Base/Left/Right.
    resolver = MappingClassResolver.from_bytes([joins.read_bytes()])
    assert _recompute(joins, resolver) == []
    result = _run(classes, "Joins")
    assert result.returncode == 0, result.stderr
    assert result.stdout.strip() == "83"


def test_unsatisfiable_merges_raise_frame_computation_warning(tmp_path: Path):
    classes = _compile(tmp_path, "Joins", JOINS_SOURCE)
    joins = classes / "Joins.class"
    # A resolver claiming Left extends Object directly while Base is a class:
    # no frame type can then serve `chosen.value()`, and lowering says so.
    resolver = MappingClassResolver.from_models([ClassModel("Left"), ClassModel("Base")])
    caught = _recompute(joins, resolver)
    assert len(caught) == 1
    message = str(caught[0].message)
    assert "Joins.pick(ZLLeft;LRight;)I" in message
    assert "Right" in message
    assert "require Base" in message

    model = ClassModel.from_bytes(joins.read_bytes())
    with warnings.catch_warnings():
        warnings.simplefilter("error", FrameComputationWarning)
        with pytest.raises(FrameComputationWarning):
            model.to_bytes_with_options(frame_mode=FrameComputationMode.RECOMPUTE, resolver=resolver)


def test_jar_rewrite_reports_frame_warnings_before_committing(tmp_path: Path):
    classes = _compile(tmp_path, "Joins", JOINS_SOURCE)
    jar_path = tmp_path / "joins.jar"
    with zipfile.ZipFile(jar_path, "w") as archive:
        for path in sorted(classes.glob("*.class")):
            archive.write(path, path.name)
    original = jar_path.read_bytes()
    resolver = MappingClassResolver.from_models([ClassModel("Left"), ClassModel("Base")])

    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        JarFile(jar_path).rewrite(tmp_path / "out.jar", frame_mode=FrameComputationMode.RECOMPUTE, resolver=resolver)
    assert [str(w.message) for w in caught if issubclass(w.category, FrameComputationWarning)]

    with warnings.catch_warnings():
        warnings.simplefilter("error", FrameComputationWarning)
        with pytest.raises(FrameComputationWarning):
            JarFile(jar_path).rewrite(frame_mode=FrameComputationMode.RECOMPUTE, resolver=resolver)
    assert jar_path.read_bytes() == original


def _super_name(resolver: MappingClassResolver, name: str) -> str | None:
    resolved = resolver.resolve_class(name)
    assert resolved is not None
    return resolved["super_name"]


def test_resolver_duplicate_policies(tmp_path: Path):
    classes = _compile(tmp_path, "Joins", JOINS_SOURCE)
    left = (classes / "Left.class").read_bytes()
    base = (classes / "Base.class").read_bytes()
    with pytest.raises(Exception, match="duplicate resolved class name Left"):
        MappingClassResolver.from_bytes([left, left])

    hollow = ClassModel("Left", super_name="Other").to_bytes()
    first = MappingClassResolver.from_bytes([left, hollow], duplicates="first")
    last = MappingClassResolver.from_bytes([left, hollow], duplicates="last")
    assert _super_name(first, "Left") == "Base"
    assert _super_name(last, "Left") == "Other"
    assert len(first) == 1 and "Left" in first and "Base" not in first

    combined = first.merged(MappingClassResolver.from_bytes([base, hollow]), duplicates="first")
    assert len(combined) == 2
    assert _super_name(combined, "Left") == "Base"
    invalid: Any = "newest"
    with pytest.raises(ValueError, match="duplicates must be"):
        MappingClassResolver.from_bytes([left], duplicates=invalid)


def _lambda_body_name(model: ClassModel) -> str:
    return next(method.name for method in model.methods if method.name.startswith("lambda$"))


def test_bootstrap_methods_round_trip_byte_for_byte(tmp_path: Path):
    classes = _compile(tmp_path, "Lambdas", LAMBDA_SOURCE)
    data = (classes / "Lambdas.class").read_bytes()
    model = ClassModel.from_bytes(data)
    entries = model.bootstrap_methods
    assert len(entries) == 1
    entry = entries[0]
    assert entry.method.owner == "java/lang/invoke/LambdaMetafactory"
    assert [argument.value_type for argument in entry.arguments] == ["method_type", "method_handle", "method_type"]
    body = entry.arguments[1].value
    assert isinstance(body, MethodHandleValue)
    assert (body.owner, body.name) == ("Lambdas", _lambda_body_name(model))
    assert entry == BootstrapMethod(entry.method, entry.arguments)

    model.bootstrap_methods = entries
    assert model.to_bytes() == ClassModel.from_bytes(data).to_bytes()


def test_renaming_a_lambda_body_through_bootstrap_arguments_runs(tmp_path: Path):
    classes = _compile(tmp_path, "Lambdas", LAMBDA_SOURCE)
    path = classes / "Lambdas.class"
    model = ClassModel.from_bytes(path.read_bytes())
    old_name = _lambda_body_name(model)
    for method in model.methods:
        if method.name == old_name:
            method.name = "renamedBody"

    # Renaming only the declaration leaves the bootstrap argument stale.
    path.write_bytes(model.to_bytes())
    stale = _run(classes, "Lambdas")
    assert stale.returncode != 0
    assert "renamedBody" not in stale.stdout

    def retarget(argument: BootstrapArgument) -> BootstrapArgument:
        handle = argument.value
        if argument.value_type != "method_handle" or not isinstance(handle, MethodHandleValue):
            return argument
        if (handle.owner, handle.name) != ("Lambdas", old_name):
            return argument
        return BootstrapArgument.method_handle(
            handle.reference_kind, handle.owner, "renamedBody", handle.descriptor, handle.is_interface
        )

    model.bootstrap_methods = [
        BootstrapMethod(entry.method, [retarget(argument) for argument in entry.arguments])
        for entry in model.bootstrap_methods
    ]
    path.write_bytes(model.to_bytes())
    fixed = _run(classes, "Lambdas")
    assert fixed.returncode == 0, fixed.stderr
    assert fixed.stdout.strip() == "lambda-ran"

    model.bootstrap_methods = []
    assert ClassModel.from_bytes(model.to_bytes()).bootstrap_methods == []
