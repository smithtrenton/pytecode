"""Reordering and deleting class members by assigning ``ClassModel.methods`` / ``fields``.

The member views are read-only sequences; the member lists are edited by
assigning a new list (reordered, filtered, or extended). Each scenario lowers
with recomputed frames and runs on the JVM with ``-Xverify:all``.
"""

from __future__ import annotations

import subprocess
from pathlib import Path

import pytest
from helpers import _jdk_tool, compile_java_sources

from pytecode.analysis import verify_classfile
from pytecode.archive import FrameComputationMode
from pytecode.model import ClassModel

MEMBERS_SOURCE = """
public class Members {
    static int unusedField;
    static String greeting = "hello";
    static int counter;

    static void unused() {
        unusedField++;
    }

    static String greet() {
        counter++;
        return counter > 1 ? greeting + "-again" : greeting;
    }

    public static void main(String[] args) {
        System.out.println(greet() + " " + greet());
    }
}
"""


def _compile(tmp_path: Path) -> Path:
    source = tmp_path / "src" / "Members.java"
    source.parent.mkdir(parents=True)
    source.write_text(MEMBERS_SOURCE, encoding="utf-8")
    return compile_java_sources(tmp_path, [source], release=17)


def _lower_and_run(classes: Path, model: ClassModel) -> ClassModel:
    data = model.to_bytes_with_options(frame_mode=FrameComputationMode.RECOMPUTE)
    assert [d.message for d in verify_classfile(data) if d.severity == "error"] == []
    (classes / "Members.class").write_bytes(data)
    result = subprocess.run(
        [_jdk_tool("java"), "-Xverify:all", "-cp", str(classes), "Members"],
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode == 0, result.stderr
    assert result.stdout.strip() == "hello hello-again"
    return ClassModel.from_bytes(data)


def _method_names(model: ClassModel) -> list[str]:
    return [method.name for method in model.methods]


def _field_names(model: ClassModel) -> list[str]:
    return [field.name for field in model.fields]


def test_reordering_members(tmp_path: Path):
    classes = _compile(tmp_path)
    model = ClassModel.from_bytes((classes / "Members.class").read_bytes())
    methods = _method_names(model)
    fields = _field_names(model)

    model.methods = list(model.methods)[::-1]
    model.fields = list(model.fields)[::-1]
    assert _method_names(model) == methods[::-1]

    reparsed = _lower_and_run(classes, model)
    assert _method_names(reparsed) == methods[::-1]
    assert _field_names(reparsed) == fields[::-1]

    # Each method keeps its own code and frames, so preserving them works too.
    preserved = ClassModel.from_bytes(model.to_bytes_with_options(frame_mode=FrameComputationMode.PRESERVE))
    assert _method_names(preserved) == methods[::-1]


def test_filtering_the_lists_deletes_members(tmp_path: Path):
    classes = _compile(tmp_path)
    model = ClassModel.from_bytes((classes / "Members.class").read_bytes())

    model.methods = [method for method in model.methods if method.name != "unused"]
    model.fields = [field for field in model.fields if field.name != "unusedField"]

    reparsed = _lower_and_run(classes, model)
    assert "unused" not in _method_names(reparsed)
    assert _field_names(reparsed) == ["greeting", "counter"]


def test_reassigning_unchanged_lists_round_trips_byte_for_byte(tmp_path: Path):
    data = (_compile(tmp_path) / "Members.class").read_bytes()
    model = ClassModel.from_bytes(data)
    model.methods = list(model.methods)
    model.fields = list(model.fields)
    assert model.to_bytes() == data


def test_views_do_not_delete_and_earlier_handles_go_stale(tmp_path: Path):
    model = ClassModel.from_bytes((_compile(tmp_path) / "Members.class").read_bytes())
    with pytest.raises(TypeError):
        del model.methods[0]  # pyright: ignore[reportIndexIssue]
    with pytest.raises(TypeError):
        del model.fields[0]  # pyright: ignore[reportIndexIssue]

    # Assignment copies the members in; handles taken before it are stale.
    before = model.methods[0]
    model.methods = list(model.methods)
    with pytest.raises(RuntimeError, match="stale"):
        _ = before.name
    assert model.methods[0].name == "<init>"
