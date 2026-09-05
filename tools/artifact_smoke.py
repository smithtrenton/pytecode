"""Exercise an installed distribution with no checkout or Java dependency.

Run using the installed environment's Python with ``-I`` from outside the repo.
"""

from __future__ import annotations

import importlib.metadata
import sys
import tempfile
import zipfile
from pathlib import Path

import pytecode
from pytecode import _rust
from pytecode.archive import FrameComputationMode, JarFile
from pytecode.classfile import ClassReader, ClassWriter
from pytecode.model import ClassModel, MethodModel


def main() -> None:
    """Check distribution files, native loading, edits, frames, and ZIP output."""
    package = Path(pytecode.__file__).resolve().parent
    assert package.is_relative_to(Path(sys.prefix).resolve()), package
    assert (package / "py.typed").is_file()
    assert (package / "_rust.pyi").is_file()
    files = importlib.metadata.files("pytecode")
    assert files is not None and any("license" in str(path).lower() for path in files)
    print(f"Python: {sys.version}; extension: {_rust.__file__}")

    # Independent minimal class: public Artifact extends java/lang/Object, no members.
    data = bytes.fromhex(
        "cafebabe 0000 0034 0005 010008 4172746966616374 070001 "
        "010010 6a6176612f6c616e672f4f626a656374 070003 "
        "0021 0002 0004 0000 0000 0000 0000"
    )
    assert ClassWriter.write(ClassReader.from_bytes(data).class_info) == data
    model = ClassModel.from_bytes(data)
    model.name = "InstalledArtifact"
    method = MethodModel("answer", "()I", 0x0009)
    method.set_raw_code(1, 0, bytes.fromhex("10 2a 74 ac"))
    model.methods = [method]
    emitted = model.to_bytes_with_options(frame_mode=FrameComputationMode.RECOMPUTE)
    reloaded = ClassModel.from_bytes(emitted)
    assert reloaded.name == "InstalledArtifact"
    assert reloaded.methods[0].name == "answer"
    text = "\x00\ud800value\udc00"
    pool = _rust.ConstantPoolBuilder()
    assert pool.resolve_utf8(pool.add_utf8(text)) == text
    assert _rust.LdcInsn.string(text).value == text

    with tempfile.TemporaryDirectory(prefix="pytecode-smoke-") as directory:
        path = Path(directory) / "artifact.jar"
        with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
            archive.comment = b"installed-wheel"
            archive.writestr("InstalledArtifact.class", emitted)
            archive.writestr("resource.txt", b"before")
        jar = JarFile(path)
        jar.files["resource.txt"].bytes = b"after"
        jar.rewrite(frame_mode=FrameComputationMode.RECOMPUTE)
        with zipfile.ZipFile(path) as archive:
            assert archive.comment == b"installed-wheel"
            assert archive.read("resource.txt") == b"after"
            assert ClassModel.from_bytes(archive.read("InstalledArtifact.class")).name == "InstalledArtifact"
    print("Installed artifact checks passed.")


if __name__ == "__main__":
    main()
