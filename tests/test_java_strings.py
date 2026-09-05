"""Java string values preserve UTF-16 code units across Python and Rust."""

import subprocess
from pathlib import Path

import pytest
from helpers import _jdk_tool, compile_java_sources

from pytecode import _rust
from pytecode.analysis import verify_classfile
from pytecode.archive import FrameComputationMode
from pytecode.classfile import ClassReader, ClassWriter


@pytest.mark.parametrize("value", ["", "plain", "\x00", "\ud800", "\udc00", "A\x00\ud800B\udc00😀"])
def test_python_string_and_constant_pool_roundtrip(value: str):
    assert _rust.LdcInsn.string(value).value == value
    pool = _rust.ConstantPoolBuilder()
    index = pool.add_utf8(value)
    assert pool.resolve_utf8(index) == value
    assert pool.add_utf8(value) == index
    assert pool.add_string(value) == pool.add_string(value)


def test_javac_surrogate_literals_roundtrip_and_execute(tmp_path: Path):
    source = Path(__file__).resolve().parents[1] / "crates/pytecode-engine/fixtures/java/SurrogateStrings.java"
    classes = compile_java_sources(tmp_path, [source])
    for path in classes.rglob("*.class"):
        original = path.read_bytes()
        raw = ClassReader.from_bytes(original).class_info
        assert ClassWriter.write(raw) == original
        assert not [item for item in verify_classfile(original) if item.severity == "error"]
        model = _rust.ClassModel.from_bytes(original)
        path.write_bytes(model.to_bytes_with_options(frame_mode=FrameComputationMode.RECOMPUTE))
    result = subprocess.run(
        [_jdk_tool("java"), "-Xverify:all", "-cp", str(classes), "SurrogateStrings"],
        check=True,
        capture_output=True,
        text=True,
    )
    assert result.stdout.strip() == "surrogates-ok"
