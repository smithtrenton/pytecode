"""A required JDK 26 CI lane; opt in locally with PYTECODE_TEST_JAVA26=1."""

import os
import subprocess
from pathlib import Path

import pytest
from helpers import _jdk_tool, compile_java_sources

from pytecode.archive import FrameComputationMode
from pytecode.classfile import ClassReader, ClassWriter
from pytecode.model import ClassModel

pytestmark = pytest.mark.skipif(os.environ.get("PYTECODE_TEST_JAVA26") != "1", reason="dedicated JDK 26 lane")


def test_java26_roundtrip_recompute_and_execute(tmp_path: Path):
    root = Path("crates/pytecode-engine/fixtures/java")
    classes = compile_java_sources(tmp_path, [root / "Java25Features.java", root / "FrameRegression.java"], release=26)
    for path in classes.rglob("*.class"):
        original = path.read_bytes()
        raw = ClassReader.from_bytes(original).class_info
        assert raw.major_version == 70
        assert ClassWriter.write(raw) == original
        path.write_bytes(
            ClassModel.from_bytes(original).to_bytes_with_options(frame_mode=FrameComputationMode.RECOMPUTE)
        )
    for main, expected in [("Java25Features", "amount = 99.5"), ("FrameRegression", "frames-ok")]:
        result = subprocess.run(
            [_jdk_tool("java"), "-Xverify:all", "-cp", str(classes), main],
            check=True,
            capture_output=True,
            text=True,
        )
        assert expected in result.stdout


def test_java26_preview_requires_matching_runtime_and_flag(tmp_path: Path):
    source = tmp_path / "Preview26.java"
    source.write_text(
        "public class Preview26 {"
        " static int value(Object x) { return x instanceof int n ? n : -1; }"
        " public static void main(String[] args) {"
        " if (value(Integer.valueOf(42)) != 42) throw new AssertionError();"
        ' System.out.println("preview-ok"); } }',
        encoding="utf-8",
    )
    subprocess.run(
        [_jdk_tool("javac"), "--enable-preview", "--release", "26", str(source)], check=True, capture_output=True
    )
    path = source.with_suffix(".class")
    model = ClassModel.from_bytes(path.read_bytes())
    path.write_bytes(model.to_bytes_with_options(frame_mode=FrameComputationMode.RECOMPUTE))
    raw = ClassReader.from_bytes(path.read_bytes()).class_info
    assert (raw.major_version, raw.minor_version) == (70, 65535)
    base = ["-Xverify:all", "-cp", str(tmp_path), "Preview26"]
    result = subprocess.run([_jdk_tool("java"), "--enable-preview", *base], check=True, capture_output=True, text=True)
    assert result.stdout.strip() == "preview-ok"
    without_flag = subprocess.run([_jdk_tool("java"), *base], capture_output=True, text=True)
    assert without_flag.returncode != 0
