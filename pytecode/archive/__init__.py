"""Read, modify, and rewrite JAR archives with optional bytecode transformation."""

from __future__ import annotations

import copy
import os
import tempfile
import zipfile
from dataclasses import dataclass
from pathlib import Path, PurePosixPath

from ..analysis.hierarchy import ClassResolver
from ..classfile.reader import ClassReader
from ..edit.debug_info import DebugInfoPolicy, normalize_debug_info_policy
from ..edit.model import ClassModel
from ..transforms import ClassTransform

__all__ = ["JarFile", "JarInfo"]


@dataclass
class JarInfo:
    """Metadata and raw content for a single entry in a JAR archive.

    Attributes:
        filename: Normalized, OS-native relative path of the entry.
        zipinfo: Original ZIP central-directory header for the entry.
        bytes: Raw (uninterpreted) byte content of the entry.
    """

    filename: str
    zipinfo: zipfile.ZipInfo
    bytes: bytes


def _normalize_filename(
    filename: str | os.PathLike[str],
    *,
    is_dir: bool | None = None,
) -> str:
    raw = os.fspath(filename)
    if not raw:
        raise ValueError("JAR entry filename must not be empty")
    if raw.startswith(("/", "\\")) or Path(raw).is_absolute():
        raise ValueError(f"JAR entry filename must be relative: {raw!r}")

    posix_path = raw.replace("\\", "/")
    parts = PurePosixPath(posix_path).parts
    if ".." in parts:
        raise ValueError(f"JAR entry filename must not contain parent directory references: {raw!r}")

    normalized = str(Path(*parts))
    if normalized in ("", "."):
        raise ValueError("JAR entry filename must not be empty")

    if is_dir is None:
        is_dir = raw.endswith(("/", "\\"))
    if is_dir:
        return normalized.rstrip("\\/") + os.sep
    return normalized


def _archive_name(filename: str) -> str:
    is_dir = filename.endswith(os.sep)
    stripped = filename.rstrip("\\/")
    archive_name = PurePosixPath(*Path(stripped).parts).as_posix()
    if is_dir:
        return archive_name + "/"
    return archive_name


def _clone_zipinfo(zipinfo: zipfile.ZipInfo, *, filename: str) -> zipfile.ZipInfo:
    clone = copy.copy(zipinfo)
    clone.filename = _archive_name(filename)
    return clone


def _is_class_filename(filename: str) -> bool:
    return not filename.endswith(os.sep) and filename.endswith(".class")


def _read_archive_state(filename: str | os.PathLike[str]) -> tuple[list[zipfile.ZipInfo], dict[str, JarInfo]]:
    files: dict[str, JarInfo] = {}
    with zipfile.ZipFile(filename, "r") as jar:
        infolist = jar.infolist()
        for info in infolist:
            normalized = _normalize_filename(info.filename, is_dir=info.is_dir())
            data = b"" if info.is_dir() else jar.read(info.filename)
            files[normalized] = JarInfo(normalized, info, data)
    return infolist, files


class JarFile:
    """In-memory representation of a JAR (ZIP) archive.

    On construction the archive is read into memory so entries can be
    inspected, added, removed, and optionally transformed before being
    written back to disk via ``rewrite``.

    Signed-JAR artifacts (``META-INF/*.SF``, ``*.RSA``, etc.) are kept as
    ordinary resources and are **not** re-signed when the archive is
    rewritten.
    """

    def __init__(self, filename: str | os.PathLike[str]) -> None:
        """Open and read a JAR archive into memory.

        Args:
            filename: Path to an existing JAR file on disk.
        """
        self.filename = os.fspath(filename)
        self.infolist: list[zipfile.ZipInfo] = []
        self.files: dict[str, JarInfo] = {}
        self.read()

    def read(self) -> None:
        """Re-read the archive from disk, replacing all in-memory state."""
        self.infolist, self.files = _read_archive_state(self.filename)

    def add_file(
        self,
        filename: str | os.PathLike[str],
        data: bytes | bytearray,
        *,
        zipinfo: zipfile.ZipInfo | None = None,
    ) -> JarInfo:
        """Add or replace an entry in the archive.

        If *filename* already exists its ZIP metadata is reused unless an
        explicit *zipinfo* is supplied.

        Args:
            filename: Relative path for the entry inside the JAR.
            data: Raw bytes to store for this entry.
            zipinfo: Optional ZIP header to use instead of the default.

        Returns:
            The ``JarInfo`` for the newly added entry.
        """
        normalized = _normalize_filename(filename)
        if zipinfo is not None:
            entry_zipinfo = copy.copy(zipinfo)
        elif normalized in self.files:
            entry_zipinfo = copy.copy(self.files[normalized].zipinfo)
        else:
            entry_zipinfo = zipfile.ZipInfo()
        entry_zipinfo.filename = _archive_name(normalized)
        jar_info = JarInfo(normalized, entry_zipinfo, bytes(data))
        self.files[normalized] = jar_info
        self.infolist = [item.zipinfo for item in self.files.values()]
        return jar_info

    def remove_file(self, filename: str | os.PathLike[str]) -> JarInfo:
        """Remove an entry from the archive.

        Args:
            filename: Relative path of the entry to remove.

        Returns:
            The ``JarInfo`` that was removed.

        Raises:
            KeyError: If the entry does not exist.
        """
        normalized = _normalize_filename(filename)
        try:
            jar_info = self.files.pop(normalized)
        except KeyError as exc:
            raise KeyError(normalized) from exc
        self.infolist = [item.zipinfo for item in self.files.values()]
        return jar_info

    def parse_classes(self) -> tuple[list[tuple[JarInfo, ClassReader]], list[JarInfo]]:
        """Parse all ``.class`` entries and separate them from other resources.

        Returns:
            A two-element tuple of (class entries, non-class entries).
            Each class entry is a ``(JarInfo, ClassReader)`` pair.
        """
        classes: list[tuple[JarInfo, ClassReader]] = []
        other_files: list[JarInfo] = []
        for jar_info in self.files.values():
            if _is_class_filename(jar_info.filename):
                classes.append((jar_info, ClassReader.from_bytes(jar_info.bytes)))
            else:
                other_files.append(jar_info)
        return classes, other_files

    def rewrite(
        self,
        output_path: str | os.PathLike[str] | None = None,
        *,
        transform: ClassTransform | None = None,
        recompute_frames: bool = False,
        resolver: ClassResolver | None = None,
        debug_info: DebugInfoPolicy | str = DebugInfoPolicy.PRESERVE,
        skip_debug: bool = False,
    ) -> Path:
        """Write the current archive state back to disk.

        By default the archive is rewritten in place.  ``.class`` entries are
        copied verbatim unless *transform* or non-default lowering options
        require re-lowering through ``ClassModel``.

        Signed-JAR artifacts under ``META-INF`` are preserved as ordinary
        resources and are **not** re-signed; if class bytes change the
        resulting archive may no longer verify.

        Args:
            output_path: Destination path.  When ``None`` the original file
                is overwritten.
            transform: Optional callable applied to each ``ClassModel``
                in place.  Must return ``None``.
            recompute_frames: Whether to recompute ``StackMapTable`` frames
                when lowering classes.
            resolver: Class hierarchy resolver used during frame computation.
            debug_info: Policy controlling how debug attributes are emitted.
            skip_debug: If ``True``, discard debug attributes when lifting
                class bytes into ``ClassModel``.

        Returns:
            The resolved destination ``Path``.

        Raises:
            TypeError: If *transform* returns a non-``None`` value.
        """

        debug_policy = normalize_debug_info_policy(debug_info)
        should_rewrite_classes = (
            transform is not None
            or recompute_frames
            or resolver is not None
            or debug_policy is not DebugInfoPolicy.PRESERVE
            or skip_debug
        )

        destination = Path(self.filename if output_path is None else output_path)
        destination.parent.mkdir(parents=True, exist_ok=True)

        fd, temp_name = tempfile.mkstemp(prefix=f"{destination.name}-", suffix=".tmp", dir=destination.parent)
        os.close(fd)
        temp_path = Path(temp_name)

        try:
            with zipfile.ZipFile(temp_path, "w") as jar:
                for jar_info in self.files.values():
                    data = jar_info.bytes
                    if should_rewrite_classes and _is_class_filename(jar_info.filename):
                        model = ClassModel.from_bytes(data, skip_debug=skip_debug)
                        if transform is not None:
                            result = transform(model)
                            if result is not None:
                                raise TypeError(
                                    "JarFile.rewrite() transforms must mutate ClassModel in place and return None"
                                )
                        data = model.to_bytes(
                            recompute_frames=recompute_frames,
                            resolver=resolver,
                            debug_info=debug_policy,
                        )
                    jar.writestr(_clone_zipinfo(jar_info.zipinfo, filename=jar_info.filename), data)

            temp_path.replace(destination)
            new_infolist, new_files = _read_archive_state(destination)
            self.filename = os.fspath(destination)
            self.infolist = new_infolist
            self.files = new_files
            return destination
        finally:
            if temp_path.exists():
                temp_path.unlink()
