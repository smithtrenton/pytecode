"""Editing ``invokedynamic`` call-site and dynamic-constant descriptors.

A call site's descriptor lives in its ``CONSTANT_InvokeDynamic`` entry rather
than in any method, so a rename that rewrites method descriptors must rewrite
the call site too: a stale call-site descriptor only fails when the call site
first links.
"""

from __future__ import annotations

import subprocess
from pathlib import Path
from typing import TYPE_CHECKING, cast

import pytest
from helpers import _jdk_tool, compile_java_resource_classes, compile_java_sources

from pytecode.analysis import verify_classfile
from pytecode.archive import DebugInfoPolicy, FrameComputationMode
from pytecode.model import (
    BootstrapArgument,
    BootstrapMethod,
    ClassModel,
    DynamicValue,
    InvokeDynamicInsn,
    LdcInsn,
    MethodHandleValue,
    MethodInsn,
    TypeInsn,
)

if TYPE_CHECKING:
    from pytecode._rust import CodeItem, CodeModel

THROWING_CONSUMER = "LambdaShowcase$ThrowingConsumer"
RENAMED_CONSUMER = "LambdaShowcase$RenamedConsumer"


def _lambda_showcase(tmp_path: Path) -> bytes:
    classes = compile_java_resource_classes(tmp_path, "LambdaShowcase.java", release=17)
    return next(path for path in classes if path.name == "LambdaShowcase.class").read_bytes()


def _instructions(code: CodeModel) -> list[CodeItem]:
    """Detached copies of ``code``'s instructions, ready to edit and write back."""
    return cast("list[CodeItem]", code.instructions.to_list())


def _call_sites(model: ClassModel) -> list[InvokeDynamicInsn]:
    return [
        item
        for method in model.methods
        if method.code is not None
        for item in method.code.instructions
        if isinstance(item, InvokeDynamicInsn)
    ]


def _rewrite_call_sites(model: ClassModel, old: str, new: str) -> int:
    rewritten = 0
    for method in model.methods:
        code = method.code
        if code is None:
            continue
        items = _instructions(code)
        for item in items:
            if isinstance(item, InvokeDynamicInsn):
                item.descriptor = item.descriptor.replace(old, new)
                rewritten += 1
        code.instructions = items
    return rewritten


def _structural_errors(data: bytes) -> list[str]:
    return [diagnostic.message for diagnostic in verify_classfile(data) if diagnostic.severity == "error"]


@pytest.mark.parametrize("frame_mode", [FrameComputationMode.PRESERVE, FrameComputationMode.RECOMPUTE])
def test_call_site_descriptor_rewrite_survives_reparse(tmp_path: Path, frame_mode: FrameComputationMode):
    original = _lambda_showcase(tmp_path)
    model = ClassModel.from_bytes(original)
    before = [(site.name, site.bootstrap_method_attr_index, site.descriptor) for site in _call_sites(model)]
    assert any(THROWING_CONSUMER in descriptor for _, _, descriptor in before)

    assert _rewrite_call_sites(model, THROWING_CONSUMER, RENAMED_CONSUMER) == len(before)
    data = model.to_bytes_with_options(frame_mode=frame_mode)
    assert _structural_errors(data) == []

    after = [
        (site.name, site.bootstrap_method_attr_index, site.descriptor)
        for site in _call_sites(ClassModel.from_bytes(data))
    ]
    assert after == [
        (name, index, descriptor.replace(THROWING_CONSUMER, RENAMED_CONSUMER)) for name, index, descriptor in before
    ]


def test_reassigning_the_same_call_site_descriptor_round_trips_byte_for_byte(tmp_path: Path):
    original = _lambda_showcase(tmp_path)
    model = ClassModel.from_bytes(original)
    # Every method's instruction list is written back, including methods
    # whose preserved StackMapTable frames depend on the code being unchanged.
    assert _rewrite_call_sites(model, THROWING_CONSUMER, THROWING_CONSUMER) > 0
    assert model.to_bytes() == original


def test_call_site_descriptor_setter_edits_only_the_detached_item():
    site = InvokeDynamicInsn(0, "run", "()Ljava/lang/Runnable;")
    site.descriptor = "(La/B;)Ljava/lang/Runnable;"
    assert site.descriptor == "(La/B;)Ljava/lang/Runnable;"
    assert (site.name, site.bootstrap_method_attr_index) == ("run", 0)
    with pytest.raises(ValueError, match="method descriptor"):
        site.descriptor = "Ljava/lang/Runnable;"
    assert site.descriptor == "(La/B;)Ljava/lang/Runnable;"


def test_dynamic_constant_descriptor_is_writable_and_rewraps():
    value = DynamicValue(2, "value", "La/Old;")
    value.descriptor = "La/New;"
    with pytest.raises(ValueError, match="field descriptor"):
        value.descriptor = "()V"

    ldc = LdcInsn.from_dynamic(value)
    loaded = ldc.value
    assert ldc.value_type == "dynamic"
    assert isinstance(loaded, DynamicValue)
    assert (loaded.bootstrap_method_attr_index, loaded.name, loaded.descriptor) == (2, "value", "La/New;")

    argument = BootstrapArgument.from_dynamic(value)
    assert argument == BootstrapArgument.dynamic(2, "value", "La/New;")


def test_dynamic_bootstrap_argument_descriptor_rewrite_reencodes(tmp_path: Path):
    model = ClassModel.from_bytes(_lambda_showcase(tmp_path))
    entries = model.bootstrap_methods
    old = DynamicValue(0, "constant", "La/Old;")
    model.bootstrap_methods = [*entries, BootstrapMethod(entries[0].method, [BootstrapArgument.from_dynamic(old)])]
    built = ClassModel.from_bytes(model.to_bytes())

    def rewrite(argument: BootstrapArgument) -> BootstrapArgument:
        value = argument.value
        if not isinstance(value, DynamicValue):
            return argument
        value.descriptor = value.descriptor.replace("La/Old;", "La/New;")
        return BootstrapArgument.from_dynamic(value)

    built.bootstrap_methods = [
        BootstrapMethod(entry.method, [rewrite(argument) for argument in entry.arguments])
        for entry in built.bootstrap_methods
    ]
    reparsed = ClassModel.from_bytes(built.to_bytes())
    argument = reparsed.bootstrap_methods[-1].arguments[0]
    assert argument == BootstrapArgument.dynamic(0, "constant", "La/New;")


INDY_RENAME_SOURCE = """
class Payload {
    public String toString() { return "payload"; }
}

public class IndyRename {
    static Payload make() { return new Payload(); }

    public static void main(String[] args) {
        Payload payload = make();
        Runnable task = () -> System.out.println("got " + payload);
        task.run();
    }
}
"""


def _rename_payload(model: ClassModel, *, call_sites: bool) -> None:
    """Rename ``Payload`` to ``Renamed`` in every reference shape of ``model``.

    The lambda captures ``payload``, so ``Payload`` appears in the call site's
    descriptor ``(LPayload;)Ljava/lang/Runnable;``. ``call_sites=False``
    leaves that descriptor alone, which is what a rename pipeline had to do
    before call-site descriptors became writable.
    """

    def owner(name: str) -> str:
        return "Renamed" if name == "Payload" else name

    def descriptor(value: str) -> str:
        return value.replace("LPayload;", "LRenamed;")

    def argument(value: BootstrapArgument) -> BootstrapArgument:
        handle = value.value
        if not isinstance(handle, MethodHandleValue):
            return value
        return BootstrapArgument.method_handle(
            handle.reference_kind,
            owner(handle.owner),
            handle.name,
            descriptor(handle.descriptor),
            handle.is_interface,
        )

    model.name = owner(model.name)
    model.bootstrap_methods = [
        BootstrapMethod(entry.method, [argument(value) for value in entry.arguments])
        for entry in model.bootstrap_methods
    ]
    for method in model.methods:
        method.descriptor = descriptor(method.descriptor)
        code = method.code
        if code is None:
            continue
        items = _instructions(code)
        for position, item in enumerate(items):
            if isinstance(item, MethodInsn):
                items[position] = MethodInsn(
                    item.opcode, owner(item.owner), item.name, descriptor(item.descriptor), item.is_interface
                )
            elif isinstance(item, TypeInsn):
                items[position] = TypeInsn(item.opcode, owner(item.descriptor))
            elif isinstance(item, InvokeDynamicInsn) and call_sites:
                item.descriptor = descriptor(item.descriptor)
        code.instructions = items


def _run_renamed(tmp_path: Path, *, call_sites: bool) -> subprocess.CompletedProcess[str]:
    source = tmp_path / "src" / "IndyRename.java"
    source.parent.mkdir(parents=True, exist_ok=True)
    source.write_text(INDY_RENAME_SOURCE, encoding="utf-8")
    classes = compile_java_sources(tmp_path, [source], release=17)
    for name in ("Payload", "IndyRename"):
        path = classes / f"{name}.class"
        model = ClassModel.from_bytes(path.read_bytes())
        _rename_payload(model, call_sites=call_sites)
        path.unlink()
        data = model.to_bytes_with_options(frame_mode=FrameComputationMode.RECOMPUTE, debug_info=DebugInfoPolicy.STRIP)
        assert _structural_errors(data) == []
        (classes / f"{model.name}.class").write_bytes(data)
    return subprocess.run(
        [_jdk_tool("java"), "-Xverify:all", "-cp", str(classes), "IndyRename"],
        capture_output=True,
        text=True,
        check=False,
    )


def test_renaming_a_class_named_by_a_call_site_descriptor_runs(tmp_path: Path):
    stale = _run_renamed(tmp_path / "stale", call_sites=False)
    assert stale.returncode != 0
    assert "Payload" in stale.stderr

    renamed = _run_renamed(tmp_path / "renamed", call_sites=True)
    assert renamed.returncode == 0, renamed.stderr
    assert renamed.stdout.strip() == "got payload"
