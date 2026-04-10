from __future__ import annotations

import zipfile
from pathlib import Path
from typing import Any

import pytest

import pytecode
import pytecode.archive as jar_module
from pytecode.archive import JarFile, JarInfo
from pytecode.classfile.constants import ClassAccessFlag
from pytecode.transforms.rust import RustPipelineBuilder, add_access_flags, class_named
from tests.helpers import TEST_RESOURCES, make_compiled_jar, minimal_classfile

rust = pytest.importorskip("pytecode._rust")


def make_jar(files: dict[str, bytes], path: Path) -> JarFile:
    """Write a ZIP to *path*, return the resulting JarFile."""
    with zipfile.ZipFile(path, "w") as zf:
        for name, data in files.items():
            zf.writestr(name, data)
    return JarFile(path)


def make_zipinfo(
    filename: str,
    *,
    compress_type: int = zipfile.ZIP_STORED,
    comment: bytes = b"",
    extra: bytes = b"",
    date_time: tuple[int, int, int, int, int, int] = (2024, 1, 2, 3, 4, 6),
    external_attr: int = 0,
) -> zipfile.ZipInfo:
    info = zipfile.ZipInfo(filename, date_time=date_time)
    info.compress_type = compress_type
    info.comment = comment
    info.extra = extra
    info.external_attr = external_attr
    return info


def make_extra_field(header_id: int, data: bytes) -> bytes:
    return header_id.to_bytes(2, "little") + len(data).to_bytes(2, "little") + data


def make_jar_with_infos(entries: list[tuple[zipfile.ZipInfo, bytes]], path: Path) -> JarFile:
    with zipfile.ZipFile(path, "w") as zf:
        for info, data in entries:
            zf.writestr(info, data)
    return JarFile(path)


# ---------------------------------------------------------------------------
# Basic JAR reading
# ---------------------------------------------------------------------------


def test_read_jar_files_populated(tmp_path: Path):
    jar = make_jar({"a.txt": b"hello", "b.class": minimal_classfile()}, tmp_path / "t.jar")
    assert len(jar.files) == 2


def test_jar_info_has_bytes(tmp_path: Path):
    content = b"some content"
    jar = make_jar({"readme.txt": content}, tmp_path / "t.jar")
    key = str(Path("readme.txt"))
    assert jar.files[key].bytes == content


def test_jar_info_has_zipinfo(tmp_path: Path):
    jar = make_jar({"readme.txt": b"x"}, tmp_path / "t.jar")
    key = str(Path("readme.txt"))
    assert isinstance(jar.files[key].zipinfo, zipfile.ZipInfo)


def test_jar_empty(tmp_path: Path):
    jar = make_jar({}, tmp_path / "t.jar")
    assert jar.files == {}


# ---------------------------------------------------------------------------
# parse_classes separation
# ---------------------------------------------------------------------------


def test_parse_classes_separates_class_files(tmp_path: Path):
    jar = make_jar(
        {"Foo.class": minimal_classfile(), "README.txt": b"docs"},
        tmp_path / "t.jar",
    )
    classes, others = jar.parse_classes()
    assert len(classes) == 1
    assert len(others) == 1


def test_parse_classes_all_non_class(tmp_path: Path):
    jar = make_jar(
        {"a.txt": b"a", "b.properties": b"b=1"},
        tmp_path / "t.jar",
    )
    classes, others = jar.parse_classes()
    assert classes == []
    assert len(others) == 2


def test_parse_classes_all_classes(tmp_path: Path):
    jar = make_jar(
        {"A.class": minimal_classfile(), "B.class": minimal_classfile()},
        tmp_path / "t.jar",
    )
    classes, others = jar.parse_classes()
    assert len(classes) == 2
    assert others == []


def test_parse_classes_returns_rust_classreader(tmp_path: Path):
    jar = make_jar({"Foo.class": minimal_classfile()}, tmp_path / "t.jar")
    classes, _ = jar.parse_classes()
    jar_info, cr = classes[0]
    assert isinstance(jar_info, JarInfo)
    assert isinstance(cr, pytecode.ClassReader)


def test_parse_classes_classreader_has_class_info(tmp_path: Path):
    jar = make_jar({"Foo.class": minimal_classfile()}, tmp_path / "t.jar")
    classes, _ = jar.parse_classes()
    _, cr = classes[0]
    assert cr.class_info is not None


# ---------------------------------------------------------------------------
# Path normalization
# ---------------------------------------------------------------------------


def test_path_normalization(tmp_path: Path):
    jar = make_jar(
        {"com/example/Foo.class": minimal_classfile()},
        tmp_path / "t.jar",
    )
    expected_key = str(Path("com/example/Foo.class"))
    assert expected_key in jar.files


# ---------------------------------------------------------------------------
# Integration test with compiled JAR
# ---------------------------------------------------------------------------


def test_compiled_jar_class_count(tmp_path: Path):
    jar_path = make_compiled_jar(
        tmp_path,
        [TEST_RESOURCES / "HelloWorld.java"],
        extra_files={"README.txt": b"fixture"},
    )
    jar = JarFile(jar_path)
    classes, others = jar.parse_classes()
    assert [jar_info.filename for jar_info, _ in classes] == ["HelloWorld.class"]
    assert [other.filename for other in others] == ["README.txt"]
    assert classes[0][1].class_info.methods_count >= 2


# ---------------------------------------------------------------------------
# Archive mutation helpers
# ---------------------------------------------------------------------------


def test_add_file_normalizes_path_and_updates_infolist(tmp_path: Path):
    jar = make_jar({}, tmp_path / "t.jar")
    class_bytes = minimal_classfile()

    jar.add_file("com/example/New.class", class_bytes)

    expected_key = str(Path("com/example/New.class"))
    assert jar.files[expected_key].bytes == class_bytes
    assert jar.infolist[-1].filename == "com/example/New.class"


def test_remove_file_returns_removed_entry(tmp_path: Path):
    jar = make_jar({"a.txt": b"hello"}, tmp_path / "t.jar")

    removed = jar.remove_file("a.txt")

    assert removed.filename == "a.txt"
    assert jar.files == {}


def test_remove_missing_file_raises_keyerror(tmp_path: Path):
    jar = make_jar({}, tmp_path / "t.jar")

    with pytest.raises(KeyError):
        jar.remove_file("missing.txt")


def test_add_file_rejects_parent_directory_references(tmp_path: Path):
    jar = make_jar({}, tmp_path / "t.jar")

    with pytest.raises(ValueError, match=r"parent directory references"):
        jar.add_file("nested/../../evil.txt", b"boom")


def test_read_rejects_parent_directory_references(tmp_path: Path):
    jar_path = tmp_path / "t.jar"
    with zipfile.ZipFile(jar_path, "w") as zf:
        zf.writestr("../evil.txt", b"boom")

    with pytest.raises(ValueError, match=r"parent directory references"):
        JarFile(jar_path)


def test_add_file_preserves_existing_metadata_when_replacing_entry(tmp_path: Path):
    original_info = make_zipinfo(
        "README.txt",
        compress_type=zipfile.ZIP_DEFLATED,
        comment=b"readme",
        extra=make_extra_field(0xABCD, b"R"),
        date_time=(2024, 4, 5, 6, 7, 8),
        external_attr=0x70,
    )
    jar = make_jar_with_infos([(original_info, b"old")], tmp_path / "t.jar")
    out_path = tmp_path / "out.jar"

    jar.add_file("README.txt", b"new")
    jar.rewrite(out_path)

    with zipfile.ZipFile(out_path, "r") as zf:
        info = zf.infolist()[0]
        assert info.filename == "README.txt"
        assert info.comment == b"readme"
        assert info.extra == make_extra_field(0xABCD, b"R")
        assert info.compress_type == zipfile.ZIP_DEFLATED
        assert info.date_time == (2024, 4, 5, 6, 7, 8)
        assert info.external_attr == 0x70
        assert zf.read("README.txt") == b"new"


# ---------------------------------------------------------------------------
# Rewrite support
# ---------------------------------------------------------------------------


def test_rewrite_preserves_order_and_selected_metadata(tmp_path: Path):
    manifest_bytes = b"Manifest-Version: 1.0\r\n\r\n"
    service_bytes = b"pkg.Foo\n"
    class_bytes = minimal_classfile()
    entries = [
        (
            make_zipinfo(
                "META-INF/",
                external_attr=0x10,
            ),
            b"",
        ),
        (
            make_zipinfo(
                "META-INF/MANIFEST.MF",
                comment=b"manifest",
                extra=make_extra_field(0xCAFE, b"M"),
                external_attr=0x20,
            ),
            manifest_bytes,
        ),
        (
            make_zipinfo(
                "META-INF/services/example.Service",
                compress_type=zipfile.ZIP_DEFLATED,
                comment=b"service",
                extra=make_extra_field(0xBEEF, b"OK"),
                date_time=(2024, 2, 3, 4, 5, 6),
                external_attr=0x40,
            ),
            service_bytes,
        ),
        (
            make_zipinfo(
                "pkg/Foo.class",
                compress_type=zipfile.ZIP_DEFLATED,
                comment=b"class",
                extra=make_extra_field(0xD00D, b"C"),
                date_time=(2024, 3, 4, 5, 6, 8),
                external_attr=0x60,
            ),
            class_bytes,
        ),
    ]
    jar = make_jar_with_infos(entries, tmp_path / "t.jar")
    out_path = tmp_path / "out.jar"

    jar.rewrite(out_path)

    with zipfile.ZipFile(out_path, "r") as zf:
        infos = zf.infolist()
        assert [info.filename for info in infos] == [
            "META-INF/",
            "META-INF/MANIFEST.MF",
            "META-INF/services/example.Service",
            "pkg/Foo.class",
        ]
        assert infos[0].is_dir()

        assert infos[1].comment == b"manifest"
        assert infos[1].extra == make_extra_field(0xCAFE, b"M")
        assert infos[1].compress_type == zipfile.ZIP_STORED
        assert infos[1].date_time == (2024, 1, 2, 3, 4, 6)
        assert infos[1].external_attr == 0x20

        assert infos[2].comment == b"service"
        assert infos[2].extra == make_extra_field(0xBEEF, b"OK")
        assert infos[2].compress_type == zipfile.ZIP_DEFLATED
        assert infos[2].date_time == (2024, 2, 3, 4, 5, 6)
        assert infos[2].external_attr == 0x40

        assert infos[3].comment == b"class"
        assert infos[3].extra == make_extra_field(0xD00D, b"C")
        assert infos[3].compress_type == zipfile.ZIP_DEFLATED
        assert infos[3].date_time == (2024, 3, 4, 5, 6, 8)
        assert infos[3].external_attr == 0x60

        assert zf.read("META-INF/MANIFEST.MF") == manifest_bytes
        assert zf.read("META-INF/services/example.Service") == service_bytes
        assert zf.read("pkg/Foo.class") == class_bytes


def test_rewrite_applies_rust_pipeline_transform(tmp_path: Path):
    jar_path = make_compiled_jar(
        tmp_path,
        [TEST_RESOURCES / "HelloWorld.java"],
        extra_files={"README.txt": b"fixture"},
    )
    jar = JarFile(jar_path)
    out_path = tmp_path / "rewritten-rust.jar"
    pipeline = (
        RustPipelineBuilder()
        .on_classes(
            class_named("HelloWorld"),
            add_access_flags(int(ClassAccessFlag.FINAL)),
        )
        .build()
    )

    jar.rewrite(out_path, transform=pipeline.apply)

    rewritten = JarFile(out_path)
    classes, others = rewritten.parse_classes()
    assert [jar_info.filename for jar_info, _ in classes] == ["HelloWorld.class"]
    assert ClassAccessFlag.FINAL in classes[0][1].class_info.access_flags
    assert [other.filename for other in others] == ["README.txt"]


def test_rewrite_accepts_rust_pipeline_object(tmp_path: Path):
    jar_path = make_compiled_jar(tmp_path, [TEST_RESOURCES / "HelloWorld.java"])
    jar = JarFile(jar_path)
    out_path = tmp_path / "rewritten-rust-object.jar"
    pipeline = (
        RustPipelineBuilder()
        .on_classes(
            class_named("HelloWorld"),
            add_access_flags(int(ClassAccessFlag.FINAL)),
        )
        .build()
    )

    jar.rewrite(out_path, transform=pipeline)

    rewritten = JarFile(out_path)
    class_info = pytecode.ClassReader.from_bytes(rewritten.files["HelloWorld.class"].bytes).class_info
    assert ClassAccessFlag.FINAL in class_info.access_flags


def test_rewrite_accepts_rust_class_transform_object(tmp_path: Path):
    jar_path = make_compiled_jar(tmp_path, [TEST_RESOURCES / "HelloWorld.java"])
    jar = JarFile(jar_path)
    out_path = tmp_path / "rewritten-rust-transform.jar"

    jar.rewrite(out_path, transform=add_access_flags(int(ClassAccessFlag.FINAL)))

    rewritten = JarFile(out_path)
    class_info = pytecode.ClassReader.from_bytes(rewritten.files["HelloWorld.class"].bytes).class_info
    assert ClassAccessFlag.FINAL in class_info.access_flags


def _assert_rust_rewrite_skips_python_classmodel_materialization(tmp_path: Path, transform: Any, suffix: str) -> None:
    jar_path = make_compiled_jar(tmp_path, [TEST_RESOURCES / "HelloWorld.java"])
    jar = JarFile(jar_path)
    out_path = tmp_path / f"rewritten-rust-no-python-{suffix}.jar"

    jar.rewrite(out_path, transform=transform)

    rewritten = JarFile(out_path)
    class_info = pytecode.ClassReader.from_bytes(rewritten.files["HelloWorld.class"].bytes).class_info
    assert ClassAccessFlag.FINAL in class_info.access_flags


def test_rewrite_with_rust_pipeline_object_skips_python_classmodel_materialization(
    tmp_path: Path,
) -> None:
    pipeline = (
        RustPipelineBuilder()
        .on_classes(
            class_named("HelloWorld"),
            add_access_flags(int(ClassAccessFlag.FINAL)),
        )
        .build()
    )

    _assert_rust_rewrite_skips_python_classmodel_materialization(
        tmp_path,
        pipeline,
        "object",
    )


def test_rewrite_with_rust_pipeline_apply_skips_python_classmodel_materialization(
    tmp_path: Path,
) -> None:
    pipeline = (
        RustPipelineBuilder()
        .on_classes(
            class_named("HelloWorld"),
            add_access_flags(int(ClassAccessFlag.FINAL)),
        )
        .build()
    )

    _assert_rust_rewrite_skips_python_classmodel_materialization(
        tmp_path,
        pipeline.apply,
        "bound-method",
    )


def test_rewrite_delegates_unchanged_rust_pipeline_archives_to_rust_archive_path(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    jar_path = make_compiled_jar(tmp_path, [TEST_RESOURCES / "HelloWorld.java"])
    jar = JarFile(jar_path)
    out_path = tmp_path / "delegated-rust.jar"
    pipeline = (
        RustPipelineBuilder()
        .on_classes(
            class_named("HelloWorld"),
            add_access_flags(int(ClassAccessFlag.FINAL)),
        )
        .build()
    )
    original = jar_module._rust.rewrite_archive_with_rust_transform
    calls: list[tuple[object, ...]] = []

    def wrapped(*args: Any, **kwargs: Any) -> Any:
        calls.append((*args, kwargs))
        return original(*args, **kwargs)

    monkeypatch.setattr(jar_module._rust, "rewrite_archive_with_rust_transform", wrapped)

    jar.rewrite(out_path, transform=pipeline)

    assert len(calls) == 1


def test_rewrite_keeps_python_archive_path_after_in_memory_archive_edits(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    jar_path = make_compiled_jar(tmp_path, [TEST_RESOURCES / "HelloWorld.java"])
    jar = JarFile(jar_path)
    jar.add_file("README.txt", b"updated")
    out_path = tmp_path / "python-fallback.jar"
    pipeline = (
        RustPipelineBuilder()
        .on_classes(
            class_named("HelloWorld"),
            add_access_flags(int(ClassAccessFlag.FINAL)),
        )
        .build()
    )

    def fail(*args: Any, **kwargs: Any) -> Any:
        raise AssertionError("Rust archive delegate should not run after in-memory archive edits")

    monkeypatch.setattr(jar_module._rust, "rewrite_archive_with_rust_transform", fail)

    jar.rewrite(out_path, transform=pipeline)

    rewritten = JarFile(out_path)
    assert rewritten.files["README.txt"].bytes == b"updated"


def test_rewrite_skip_debug_omits_debug_metadata_from_rewritten_classes(tmp_path: Path):
    jar_path = make_compiled_jar(tmp_path, [TEST_RESOURCES / "HelloWorld.java"])
    jar = JarFile(jar_path)
    out_path = tmp_path / "skip-debug.jar"

    jar.rewrite(out_path, skip_debug=True)

    rewritten = JarFile(out_path)
    class_info = pytecode.ClassReader.from_bytes(rewritten.files["HelloWorld.class"].bytes).class_info
    assert not any(
        isinstance(attr, (rust.SourceFileAttr, rust.SourceDebugExtensionAttr)) for attr in class_info.attributes
    )
    for method in class_info.methods:
        code_attr = next((attr for attr in method.attributes if isinstance(attr, rust.CodeAttr)), None)
        if code_attr is None:
            continue
        assert not any(
            isinstance(attr, (rust.LineNumberTableAttr, rust.LocalVariableTableAttr, rust.LocalVariableTypeTableAttr))
            for attr in code_attr.attributes
        )


def test_rewrite_rejects_plain_python_transform(tmp_path: Path) -> None:
    jar_path = make_compiled_jar(tmp_path, [TEST_RESOURCES / "HelloWorld.java"])
    jar = JarFile(jar_path)
    with pytest.raises(TypeError, match="requires a RustClassTransform"):
        jar.rewrite(transform=lambda model: None)


def test_rewrite_preserves_signature_artifacts_as_pass_through_resources(tmp_path: Path):
    jar_path = make_compiled_jar(
        tmp_path,
        [TEST_RESOURCES / "HelloWorld.java"],
        extra_files={
            "META-INF/TEST.SF": b"signature-file",
            "META-INF/TEST.RSA": b"signature-block",
        },
    )
    jar = JarFile(jar_path)
    out_path = tmp_path / "signed-out.jar"

    pipeline = (
        RustPipelineBuilder()
        .on_classes(
            class_named("HelloWorld"),
            add_access_flags(int(ClassAccessFlag.FINAL)),
        )
        .build()
    )
    jar.rewrite(out_path, transform=pipeline)

    rewritten = JarFile(out_path)
    assert rewritten.files[str(Path("META-INF/TEST.SF"))].bytes == b"signature-file"
    assert rewritten.files[str(Path("META-INF/TEST.RSA"))].bytes == b"signature-block"


def test_rewrite_rejects_skip_debug_with_rust_pipeline_transform(tmp_path: Path):
    jar_path = make_compiled_jar(tmp_path, [TEST_RESOURCES / "HelloWorld.java"])
    jar = JarFile(jar_path)
    pipeline = (
        RustPipelineBuilder()
        .on_classes(
            class_named("HelloWorld"),
            add_access_flags(int(ClassAccessFlag.FINAL)),
        )
        .build()
    )

    with pytest.raises(ValueError, match="skip_debug is not supported with Rust-backed transforms"):
        jar.rewrite(transform=pipeline, skip_debug=True)


def test_rewrite_serializes_added_and_removed_entries(tmp_path: Path):
    jar = make_jar({"Foo.class": minimal_classfile(), "README.txt": b"docs"}, tmp_path / "t.jar")

    jar.add_file("nested/new.txt", b"new")
    jar.add_file("Added.class", minimal_classfile())
    removed = jar.remove_file("README.txt")
    out_path = tmp_path / "out.jar"

    assert removed.bytes == b"docs"

    jar.rewrite(out_path)

    assert Path(jar.filename) == out_path
    assert str(Path("README.txt")) not in jar.files
    assert str(Path("nested/new.txt")) in jar.files
    assert "Added.class" in jar.files

    rewritten = JarFile(out_path)
    classes, others = rewritten.parse_classes()
    assert sorted(jar_info.filename for jar_info, _ in classes) == ["Added.class", "Foo.class"]
    assert [other.filename for other in others] == [str(Path("nested/new.txt"))]

    with zipfile.ZipFile(out_path, "r") as zf:
        assert "nested/new.txt" in zf.namelist()


def test_rewrite_is_atomic_when_transform_fails(tmp_path: Path):
    jar_path = make_compiled_jar(
        tmp_path,
        [TEST_RESOURCES / "HelloWorld.java"],
        extra_files={"README.txt": b"fixture"},
    )
    jar = JarFile(jar_path)
    original_bytes = jar_path.read_bytes()

    def explode(model: object) -> None:
        raise RuntimeError("boom")

    pipeline = RustPipelineBuilder().on_classes_custom(rust.RustClassMatcher.any(), explode).build()

    with pytest.raises(RuntimeError, match="boom"):
        jar.rewrite(transform=pipeline)

    assert jar_path.read_bytes() == original_bytes


def test_rewrite_preserves_in_memory_state_when_post_write_refresh_fails(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
):
    jar_path = make_compiled_jar(
        tmp_path,
        [TEST_RESOURCES / "HelloWorld.java"],
        extra_files={"README.txt": b"fixture"},
    )
    jar = JarFile(jar_path)
    original_filename = jar.filename
    original_keys = list(jar.files)

    real_read_archive_state = jar_module._read_archive_state
    should_fail = True

    def flaky_read_archive_state(filename: str | Path):
        nonlocal should_fail
        if should_fail:
            should_fail = False
            raise OSError("refresh failed")
        return real_read_archive_state(filename)

    monkeypatch.setattr(jar_module, "_read_archive_state", flaky_read_archive_state)

    with pytest.raises(OSError, match="refresh failed"):
        jar.rewrite()

    assert jar.filename == original_filename
    assert list(jar.files) == original_keys
