from __future__ import annotations

import zipfile
from pathlib import Path

from pytecode.class_reader import ClassReader
from pytecode.jar import JarFile, JarInfo
from tests.helpers import TEST_RESOURCES, make_compiled_jar, minimal_classfile


def make_jar(files: dict[str, bytes], path: Path) -> JarFile:
    """Write a ZIP to *path*, return the resulting JarFile."""
    with zipfile.ZipFile(path, "w") as zf:
        for name, data in files.items():
            zf.writestr(name, data)
    return JarFile(path)


# ---------------------------------------------------------------------------
# Basic JAR reading
# ---------------------------------------------------------------------------


def test_read_jar_files_populated(tmp_path):
    jar = make_jar({"a.txt": b"hello", "b.class": minimal_classfile()}, tmp_path / "t.jar")
    assert len(jar.files) == 2


def test_jar_info_has_bytes(tmp_path):
    content = b"some content"
    jar = make_jar({"readme.txt": content}, tmp_path / "t.jar")
    key = str(Path("readme.txt"))
    assert jar.files[key].bytes == content


def test_jar_info_has_zipinfo(tmp_path):
    jar = make_jar({"readme.txt": b"x"}, tmp_path / "t.jar")
    key = str(Path("readme.txt"))
    assert isinstance(jar.files[key].zipinfo, zipfile.ZipInfo)


def test_jar_empty(tmp_path):
    jar = make_jar({}, tmp_path / "t.jar")
    assert jar.files == {}


# ---------------------------------------------------------------------------
# parse_classes separation
# ---------------------------------------------------------------------------


def test_parse_classes_separates_class_files(tmp_path):
    jar = make_jar(
        {"Foo.class": minimal_classfile(), "README.txt": b"docs"},
        tmp_path / "t.jar",
    )
    classes, others = jar.parse_classes()
    assert len(classes) == 1
    assert len(others) == 1


def test_parse_classes_all_non_class(tmp_path):
    jar = make_jar(
        {"a.txt": b"a", "b.properties": b"b=1"},
        tmp_path / "t.jar",
    )
    classes, others = jar.parse_classes()
    assert classes == []
    assert len(others) == 2


def test_parse_classes_all_classes(tmp_path):
    jar = make_jar(
        {"A.class": minimal_classfile(), "B.class": minimal_classfile()},
        tmp_path / "t.jar",
    )
    classes, others = jar.parse_classes()
    assert len(classes) == 2
    assert others == []


def test_parse_classes_returns_classreader(tmp_path):
    jar = make_jar({"Foo.class": minimal_classfile()}, tmp_path / "t.jar")
    classes, _ = jar.parse_classes()
    jar_info, cr = classes[0]
    assert isinstance(jar_info, JarInfo)
    assert isinstance(cr, ClassReader)


def test_parse_classes_classreader_has_class_info(tmp_path):
    jar = make_jar({"Foo.class": minimal_classfile()}, tmp_path / "t.jar")
    classes, _ = jar.parse_classes()
    _, cr = classes[0]
    assert cr.class_info is not None


# ---------------------------------------------------------------------------
# Path normalization
# ---------------------------------------------------------------------------


def test_path_normalization(tmp_path):
    jar = make_jar(
        {"com/example/Foo.class": minimal_classfile()},
        tmp_path / "t.jar",
    )
    expected_key = str(Path("com/example/Foo.class"))
    assert expected_key in jar.files


# ---------------------------------------------------------------------------
# Integration test with compiled JAR
# ---------------------------------------------------------------------------


def test_compiled_jar_class_count(tmp_path):
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
