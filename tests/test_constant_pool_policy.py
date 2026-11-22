"""Canonical constant-pool emission (``ConstantPoolPolicy.CANONICAL``).

Classes with identical content but different pool histories lower to
identical bytes under the canonical policy; the default ``PRESERVE`` policy
keeps the original bytes.
"""

from __future__ import annotations

import subprocess
from pathlib import Path
from typing import TYPE_CHECKING, cast

import pytest
from helpers import _jdk_tool, compile_java_resource_classes, compile_java_sources

from pytecode import _rust
from pytecode.analysis import verify_classfile
from pytecode.archive import ConstantPoolPolicy as ArchiveConstantPoolPolicy
from pytecode.archive import FrameComputationMode
from pytecode.classfile import ClassWriter
from pytecode.model import ClassModel, ConstantPoolPolicy, LdcInsn

if TYPE_CHECKING:
    from pytecode._rust import CodeItem


def _lambda_showcase(tmp_path: Path) -> bytes:
    classes = compile_java_resource_classes(tmp_path, "LambdaShowcase.java", release=17)
    return next(path for path in classes if path.name == "LambdaShowcase.class").read_bytes()


def _step(data: bytes, old: str, new: str) -> bytes:
    """One edit-history step: rename method ``old`` to ``new`` and lower."""
    model = ClassModel.from_bytes(data)
    method = next(method for method in model.methods if method.name == old)
    method.name = new
    return model.to_bytes()


def _canonical(data: bytes, frame_mode: FrameComputationMode = FrameComputationMode.PRESERVE) -> bytes:
    return ClassModel.from_bytes(data).to_bytes_with_options(
        frame_mode=frame_mode, constant_pool=ConstantPoolPolicy.CANONICAL
    )


def _structural_errors(data: bytes) -> list[str]:
    return [diagnostic.message for diagnostic in verify_classfile(data) if diagnostic.severity == "error"]


def test_two_edit_histories_lower_to_identical_canonical_bytes(tmp_path: Path):
    original = _lambda_showcase(tmp_path)

    # Same final content, reached in a different order and (for A) through a
    # temporary name, so the preserving pools differ in order and orphans.
    history_a = _step(_step(_step(original, "prefixer", "temporary"), "temporary", "first"), "longerThan", "second")
    history_b = _step(_step(original, "longerThan", "second"), "prefixer", "first")
    assert history_a != history_b

    canonical_a = _canonical(history_a)
    assert canonical_a == _canonical(history_b)
    assert _structural_errors(canonical_a) == []
    assert len(canonical_a) < len(history_a)
    assert sorted(method.name for method in ClassModel.from_bytes(canonical_a).methods) == sorted(
        method.name for method in ClassModel.from_bytes(history_a).methods
    )

    # The default policy is unchanged.
    model = ClassModel.from_bytes(original)
    assert model.to_bytes() == original
    assert model.to_bytes_with_options(constant_pool=ConstantPoolPolicy.PRESERVE) == original


def test_orphaned_pool_entries_do_not_survive_canonical_emission(tmp_path: Path):
    original = _lambda_showcase(tmp_path)
    padded = ClassModel.from_bytes(original)
    padded.constant_pool.add_utf8("left over from an earlier edit")
    padded.constant_pool.add_class("gone/Away")
    assert padded.to_bytes() != original
    assert padded.to_bytes_with_options(constant_pool=ConstantPoolPolicy.CANONICAL) == _canonical(original)


def test_policy_accepts_enum_and_string_values(tmp_path: Path):
    model = ClassModel.from_bytes(_lambda_showcase(tmp_path))
    expected = model.to_bytes_with_options(constant_pool=ConstantPoolPolicy.CANONICAL)
    assert ArchiveConstantPoolPolicy is ConstantPoolPolicy
    assert model.to_bytes_with_options(constant_pool="canonical") == expected
    assert ClassWriter.write(model.to_classfile_with_options(constant_pool="canonical")) == expected
    assert _rust.rust_lower_classmodels_to_bytes([model], constant_pool="canonical") == [expected]
    assert [
        ClassWriter.write(classfile) for classfile in _rust.rust_lower_classmodels([model], constant_pool="canonical")
    ] == [expected]
    with pytest.raises(ValueError, match="constant pool policy"):
        model.to_bytes_with_options(constant_pool="sorted")
    with pytest.raises(TypeError, match="ConstantPoolPolicy"):
        model.to_bytes_with_options(constant_pool=1)  # pyright: ignore[reportArgumentType]


PROGRAM_SOURCE = """
import java.util.ArrayList;
import java.util.List;
import java.util.function.Function;

public class Canonical {
    static final String[] WORDS = {"alpha", "beta", "gamma"};

    static int score(String word) {
        int total = 0;
        for (int i = 0; i < word.length(); i++) {
            total += word.charAt(i) == 'a' ? 3 : 1;
            if (word.equals("zeta")) {
                total += 100;
            }
        }
        return total;
    }

    public static void main(String[] args) {
        List<String> out = new ArrayList<>();
        Function<String, String> tag = word -> word + ":" + score(word);
        for (String word : WORDS) {
            try {
                out.add(tag.apply(word));
            } catch (RuntimeException failure) {
                out.add("failed");
            }
        }
        System.out.println(String.join(",", out) + " " + 1234567890123L + " " + 2.5d);
    }
}
"""


def _run(classes: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [_jdk_tool("java"), "-Xverify:all", "-cp", str(classes), "Canonical"],
        capture_output=True,
        text=True,
        check=False,
    )


def _shout_zeta(model: ClassModel) -> None:
    """Rewrite ``ldc "zeta"`` in ``score`` to ``"ZETA"`` (no word matches either)."""
    code = next(method for method in model.methods if method.name == "score").code
    assert code is not None
    items = cast("list[CodeItem]", code.instructions.to_list())
    position = next(index for index, item in enumerate(items) if isinstance(item, LdcInsn) and item.value == "zeta")
    items[position] = LdcInsn.string("ZETA")
    code.instructions = items


@pytest.mark.parametrize("frame_mode", [FrameComputationMode.PRESERVE, FrameComputationMode.RECOMPUTE])
def test_canonical_output_runs_on_the_jvm(tmp_path: Path, frame_mode: FrameComputationMode):
    source = tmp_path / "src" / "Canonical.java"
    source.parent.mkdir(parents=True)
    source.write_text(PROGRAM_SOURCE, encoding="utf-8")
    classes = compile_java_sources(tmp_path, [source], release=17)
    expected = _run(classes)
    assert expected.returncode == 0, expected.stderr

    path = classes / "Canonical.class"
    original = path.read_bytes()

    # Two histories of the same edit. A grows the pool past 255 entries
    # first, so the new "ZETA" constant needs `ldc_w`; B adds it while a
    # narrow `ldc` still reaches it. The code layouts (and frames) differ.
    crowded = ClassModel.from_bytes(original)
    for index in range(300):
        crowded.constant_pool.add_utf8(f"filler{index}")
    _shout_zeta(crowded)
    history_a = crowded.to_bytes_with_options(frame_mode=FrameComputationMode.RECOMPUTE)
    direct = ClassModel.from_bytes(original)
    _shout_zeta(direct)
    history_b = direct.to_bytes_with_options(frame_mode=FrameComputationMode.RECOMPUTE)
    assert len(history_a) != len(history_b)

    # With PRESERVE, canonical emission relocates the lifted frames to the
    # new layout; with RECOMPUTE, it recomputes them.
    canonical = _canonical(history_a, frame_mode=frame_mode)
    assert canonical == _canonical(history_b, frame_mode=frame_mode)
    assert _structural_errors(canonical) == []

    path.write_bytes(canonical)
    result = _run(classes)
    assert result.returncode == 0, result.stderr
    assert result.stdout == expected.stdout
