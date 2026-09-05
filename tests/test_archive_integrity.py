"""Archive mutations and failures observed through an independent ZIP reader."""

import zipfile
from pathlib import Path

import pytest

from pytecode import JarFile


def archive(path: Path, names: tuple[str, ...] = ("one.txt", "two.txt")) -> Path:
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as writer:
        writer.comment = b"archive comment \xff\x00"
        for name in names:
            writer.writestr(name, b"original")
    return path


def test_public_entry_mutations_and_archive_comment_survive(tmp_path: Path):
    path = archive(tmp_path / "input.jar")
    jar = JarFile(path)
    entry = jar.files["one.txt"]
    entry.bytes = b"replacement"
    entry.filename = "renamed.txt"
    entry.zipinfo.comment = b"edited"
    entry.zipinfo.date_time = (2025, 6, 7, 8, 9, 10)
    entry.zipinfo.compress_type = zipfile.ZIP_STORED
    entry.zipinfo.create_system = 3
    entry.zipinfo.external_attr = 0o100640 << 16
    del jar.files["two.txt"]
    jar.rewrite()
    with zipfile.ZipFile(path) as reader:
        assert reader.namelist() == ["renamed.txt"]
        assert reader.read("renamed.txt") == b"replacement"
        assert reader.comment == b"archive comment \xff\x00"
        info = reader.getinfo("renamed.txt")
        assert info.comment == b"edited"
        assert info.date_time == (2025, 6, 7, 8, 9, 10)
        assert info.compress_type == zipfile.ZIP_STORED
        assert info.create_system == 3
        assert info.external_attr >> 16 == 0o100640
    assert jar.files["renamed.txt"].bytes == b"replacement"


@pytest.mark.parametrize("names", [("a.txt", "a.txt"), ("a.txt", "./a.txt"), ("a/b", "a\\b")])
def test_duplicate_names_are_rejected(tmp_path: Path, names: tuple[str, ...]):
    if names[0].replace("\\", "/") == names[1].replace("\\", "/"):
        with pytest.warns(UserWarning, match="Duplicate name"):
            path = archive(tmp_path / "duplicates.jar", names)
    else:
        path = archive(tmp_path / "duplicates.jar", names)
    with pytest.raises(OSError, match="duplicate"):
        JarFile(path)


@pytest.mark.parametrize("limits", [{"max_entries": 1}, {"max_entry_bytes": 7}, {"max_total_bytes": 15}])
def test_read_limits(tmp_path: Path, limits: dict[str, int]):
    path = archive(tmp_path / "input.jar")
    with pytest.raises(OSError, match="limit exceeded"):
        JarFile(path, **limits)
    assert len(JarFile(path, max_entries=2, max_entry_bytes=8, max_total_bytes=16).files) == 2


def test_failed_rewrite_preserves_existing_destination_and_cleans_temp(tmp_path: Path):
    path = archive(tmp_path / "source.jar")
    destination = tmp_path / "destination.jar"
    destination.write_bytes(b"existing destination")
    jar = JarFile(path, max_entry_bytes=8)
    jar.files["one.txt"].bytes = b"too large for configured limit"
    before = {item: item.read_bytes() for item in tmp_path.iterdir()}
    with pytest.raises(OSError, match="limit exceeded"):
        jar.rewrite(destination)
    assert {item: item.read_bytes() for item in tmp_path.iterdir()} == before
    assert jar.filename == str(path)
    assert jar.files["one.txt"].bytes == b"too large for configured limit"


def test_rename_failure_cleans_temp(tmp_path: Path):
    path = archive(tmp_path / "source.jar")
    destination = tmp_path / "directory"
    destination.mkdir()
    jar = JarFile(path)
    original = path.read_bytes()
    before = set(tmp_path.iterdir())
    with pytest.raises(OSError):
        jar.rewrite(destination)
    assert set(tmp_path.iterdir()) == before
    assert path.read_bytes() == original
    assert jar.filename == str(path)


@pytest.mark.parametrize("name", ["C:/absolute", "C:relative", "../escape", "a/../../escape"])
def test_unsafe_names_rejected_on_all_platforms(tmp_path: Path, name: str):
    jar = JarFile(archive(tmp_path / "source.jar"))
    with pytest.raises(ValueError):
        jar.add_file(name, b"data")


def test_multi_release_entries_roundtrip_independently(tmp_path: Path):
    original = Path("crates/pytecode-engine/fixtures/classes/HelloWorld/HelloWorld.class").read_bytes()
    newer = bytearray(original)
    newer[6:8] = (70).to_bytes(2, "big")
    path = tmp_path / "multi-release.jar"
    entries = {
        "META-INF/MANIFEST.MF": b"Manifest-Version: 1.0\r\nMulti-Release: true\r\n\r\n",
        "HelloWorld.class": original,
        "META-INF/versions/26/HelloWorld.class": bytes(newer),
    }
    with zipfile.ZipFile(path, "w") as writer:
        for name, data in entries.items():
            writer.writestr(name, data)
    jar = JarFile(path)
    assert len(jar.parse_classes()[0]) == 2
    from pytecode.analysis import MappingClassResolver

    with pytest.raises(Exception, match="duplicate resolved class name"):
        MappingClassResolver.from_bytes([original, bytes(newer)])
    jar.rewrite()
    with zipfile.ZipFile(path) as reader:
        assert {name: reader.read(name) for name in reader.namelist()} == entries
