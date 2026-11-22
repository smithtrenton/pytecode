"""Editing a method's exception-handler table through ``CodeModel.exception_handlers``.

Handler entries are immutable values anchored to symbolic labels; the table is
edited by assigning a new list of reconstructed ``ExceptionHandler`` entries,
the same pattern as ``ClassModel.bootstrap_methods``. Each scenario lowers with
recomputed frames and runs on the JVM with ``-Xverify:all``.
"""

from __future__ import annotations

import subprocess
import warnings
from collections.abc import Callable
from pathlib import Path

import pytest
from helpers import _jdk_tool, compile_java_sources

from pytecode.analysis import verify_classfile
from pytecode.archive import FrameComputationMode, FrameComputationWarning
from pytecode.model import ClassModel, CodeModel, ExceptionHandler, Label, MalformedClassException

HANDLERS_SOURCE = """
public class Handlers {
    static String attempt(int mode) {
        try {
            if (mode == 0) throw new IllegalStateException("state");
            if (mode == 1) throw new AssertionError("assert");
            return "ok";
        } catch (IllegalStateException failure) {
            return "state-handler";
        } catch (AssertionError failure) {
            return "assertion-handler";
        }
    }

    public static void main(String[] args) {
        StringBuilder out = new StringBuilder();
        for (int mode = 0; mode < 3; mode++) {
            try {
                out.append(attempt(mode));
            } catch (Throwable escaped) {
                out.append("escaped:").append(escaped.getMessage());
            }
            out.append(' ');
        }
        System.out.println(out.toString().trim());
    }
}
"""


def _compile(tmp_path: Path) -> Path:
    source = tmp_path / "src" / "Handlers.java"
    source.parent.mkdir(parents=True)
    source.write_text(HANDLERS_SOURCE, encoding="utf-8")
    return compile_java_sources(tmp_path, [source], release=17)


def _attempt_code(model: ClassModel) -> CodeModel:
    code = next(method for method in model.methods if method.name == "attempt").code
    assert code is not None
    return code


def _handlers(code: CodeModel) -> list[ExceptionHandler]:
    handlers = [handler for handler in code.exception_handlers if isinstance(handler, ExceptionHandler)]
    assert len(handlers) == len(code.exception_handlers)
    return handlers


def _rewrite_and_run(
    tmp_path: Path, edit: Callable[[list[ExceptionHandler]], list[ExceptionHandler]]
) -> tuple[ClassModel, str]:
    """Apply ``edit`` to ``attempt``'s table, lower with RECOMPUTE, re-parse, and run."""
    classes = _compile(tmp_path)
    path = classes / "Handlers.class"
    model = ClassModel.from_bytes(path.read_bytes())
    code = _attempt_code(model)
    code.exception_handlers = edit(_handlers(code))
    with warnings.catch_warnings():
        warnings.simplefilter("error", FrameComputationWarning)
        data = model.to_bytes_with_options(frame_mode=FrameComputationMode.RECOMPUTE)
    assert [d.message for d in verify_classfile(data) if d.severity == "error"] == []
    path.write_bytes(data)
    result = subprocess.run(
        [_jdk_tool("java"), "-Xverify:all", "-cp", str(classes), "Handlers"],
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode == 0, result.stderr
    return ClassModel.from_bytes(data), result.stdout.strip()


def _retype(handler: ExceptionHandler, catch_type: str | None) -> ExceptionHandler:
    return ExceptionHandler(handler.start, handler.end, handler.handler, catch_type)


def _assert_anchored(code: CodeModel) -> None:
    """Every handler label is one of the method's instruction labels."""
    labels = [item for item in code.instructions if isinstance(item, Label)]
    for handler in _handlers(code):
        assert handler.start in labels
        assert handler.end in labels
        assert handler.handler in labels


def test_unedited_program_output(tmp_path: Path):
    _, output = _rewrite_and_run(tmp_path, lambda handlers: handlers)
    assert output == "state-handler assertion-handler ok"


def test_rewriting_a_catch_type_changes_which_exceptions_it_catches(tmp_path: Path):
    def retarget(handlers: list[ExceptionHandler]) -> list[ExceptionHandler]:
        assert [handler.catch_type for handler in handlers] == [
            "java/lang/IllegalStateException",
            "java/lang/AssertionError",
        ]
        return [_retype(handlers[0], "java/lang/IllegalArgumentException"), handlers[1]]

    reparsed, output = _rewrite_and_run(tmp_path, retarget)
    assert output == "escaped:state assertion-handler ok"
    code = _attempt_code(reparsed)
    assert [handler.catch_type for handler in _handlers(code)] == [
        "java/lang/IllegalArgumentException",
        "java/lang/AssertionError",
    ]
    _assert_anchored(code)


def test_a_catch_all_handler_catches_everything(tmp_path: Path):
    reparsed, output = _rewrite_and_run(tmp_path, lambda handlers: [_retype(handlers[0], None), *handlers[1:]])
    assert output == "state-handler state-handler ok"
    code = _attempt_code(reparsed)
    assert [handler.catch_type for handler in _handlers(code)] == [None, "java/lang/AssertionError"]
    _assert_anchored(code)


def test_filtering_the_list_deletes_a_handler(tmp_path: Path):
    reparsed, output = _rewrite_and_run(
        tmp_path,
        lambda handlers: [handler for handler in handlers if handler.catch_type != "java/lang/AssertionError"],
    )
    assert output == "state-handler escaped:assert ok"
    code = _attempt_code(reparsed)
    assert [handler.catch_type for handler in _handlers(code)] == ["java/lang/IllegalStateException"]
    _assert_anchored(code)


def test_reassigning_the_same_table_round_trips_byte_for_byte(tmp_path: Path):
    data = (_compile(tmp_path) / "Handlers.class").read_bytes()
    model = ClassModel.from_bytes(data)
    code = _attempt_code(model)
    code.exception_handlers = [_retype(handler, handler.catch_type) for handler in _handlers(code)]
    assert model.to_bytes() == data


def test_entries_are_read_only_and_labels_must_be_in_the_code(tmp_path: Path):
    model = ClassModel.from_bytes((_compile(tmp_path) / "Handlers.class").read_bytes())
    code = _attempt_code(model)
    handler = _handlers(code)[0]
    with pytest.raises(AttributeError):
        handler.catch_type = "java/lang/Throwable"  # pyright: ignore[reportAttributeAccessIssue]

    # A label that anchors nothing in the instruction list cannot be lowered.
    code.exception_handlers = [ExceptionHandler(Label(), handler.end, handler.handler, None)]
    with pytest.raises(MalformedClassException, match="label"):
        model.to_bytes_with_options(frame_mode=FrameComputationMode.RECOMPUTE)
