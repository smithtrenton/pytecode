"""Read, modify, and rewrite JAR archives with optional bytecode transformation."""

from __future__ import annotations

import copy
import os
import zipfile
from collections.abc import Callable
from dataclasses import dataclass
from enum import Enum
from pathlib import Path, PurePosixPath

from . import _rust
from .classfile import ClassReader
from .model import ClassModel

__all__ = [
    "DebugInfoPolicy",
    "FrameComputationMode",
    "JarFile",
    "JarInfo",
    "normalize_debug_info_policy",
]


class DebugInfoPolicy(Enum):
    """Policy controlling whether archive rewrites preserve or strip debug info."""

    PRESERVE = "preserve"
    STRIP = "strip"


class FrameComputationMode(Enum):
    """Policy controlling whether lowering preserves or recomputes stack map frames."""

    PRESERVE = "preserve"
    RECOMPUTE = "recompute"


_RewriteTransform = (
    _rust.ClassTransform | _rust.Pipeline | _rust.CompiledPipeline | Callable[[ClassModel], object | None]
)


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


def _is_class_filename(filename: str) -> bool:
    return not filename.endswith(os.sep) and filename.endswith(".class")


def _is_supported_transform(transform: object) -> bool:
    return isinstance(transform, (_rust.ClassTransform, _rust.Pipeline, _rust.CompiledPipeline)) or callable(transform)


def _effective_rust_debug_policy(debug_policy: DebugInfoPolicy, *, skip_debug: bool) -> str:
    return DebugInfoPolicy.STRIP.value if skip_debug else debug_policy.value


def _zipinfo_system(zipinfo: zipfile.ZipInfo) -> int:
    return int(getattr(zipinfo, "create_system", 255))


def _entry_state_from_jarinfo(
    jar_info: JarInfo,
    *,
    original_index: int | None,
) -> _rust._ArchiveEntryState:
    zipinfo = jar_info.zipinfo
    year, month, day, hour, minute, second = zipinfo.date_time
    return _rust._ArchiveEntryState(
        jar_info.filename,
        jar_info.bytes,
        int(zipinfo.compress_type),
        (year, month, day, hour, minute, second),
        system=_zipinfo_system(zipinfo),
        unix_mode=(zipinfo.external_attr >> 16) or None,
        is_dir=zipinfo.is_dir(),
        comment=bytes(zipinfo.comment),
        extra_data=bytes(zipinfo.extra),
        original_index=original_index,
    )


def _zipinfo_from_entry_state(state: _rust._ArchiveEntryState) -> zipfile.ZipInfo:
    info = zipfile.ZipInfo(_archive_name(state.filename), date_time=state.date_time)
    info.compress_type = state.compression_method
    info.comment = bytes(state.comment)
    info.extra = bytes(state.extra_data)
    info.create_system = state.system
    if state.unix_mode is not None:
        info.external_attr = int(state.unix_mode) << 16
    return info


def _jarinfo_from_entry_state(state: _rust._ArchiveEntryState) -> JarInfo:
    normalized = _normalize_filename(state.filename, is_dir=state.is_dir)
    return JarInfo(normalized, _zipinfo_from_entry_state(state), bytes(state.data))


def normalize_debug_info_policy(policy: DebugInfoPolicy | str) -> DebugInfoPolicy:
    """Normalize archive debug-info policy values."""

    if isinstance(policy, DebugInfoPolicy):
        return policy
    try:
        return DebugInfoPolicy(policy)
    except ValueError as exc:
        raise ValueError("debug_info must be one of: preserve, strip") from exc


def _read_archive_state(
    filename: str | os.PathLike[str],
) -> tuple[list[zipfile.ZipInfo], dict[str, JarInfo], dict[str, _rust._ArchiveEntryState]]:
    states = _rust.read_archive_state(os.fspath(filename))
    infolist: list[zipfile.ZipInfo] = []
    files: dict[str, JarInfo] = {}
    entry_states: dict[str, _rust._ArchiveEntryState] = {}
    for state in states:
        jar_info = _jarinfo_from_entry_state(state)
        infolist.append(jar_info.zipinfo)
        files[jar_info.filename] = jar_info
        entry_states[jar_info.filename] = state
    return infolist, files, entry_states


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
        self._entry_states: dict[str, _rust._ArchiveEntryState] = {}
        self.read()

    def read(self) -> None:
        """Re-read the archive from disk, replacing all in-memory state."""
        self.infolist, self.files, self._entry_states = _read_archive_state(self.filename)

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
        self._entry_states[normalized] = _entry_state_from_jarinfo(jar_info, original_index=None)
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
        self._entry_states.pop(normalized, None)
        self.infolist = [item.zipinfo for item in self.files.values()]
        return jar_info

    def parse_classes(self) -> tuple[list[tuple[JarInfo, ClassReader]], list[JarInfo]]:
        """Parse all ``.class`` entries into Rust-backed readers and separate them from other resources.

        Returns:
            A two-element tuple of (class entries, non-class entries).
            Each class entry is a ``(JarInfo, pytecode.ClassReader)`` pair.
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
        transform: _RewriteTransform | None = None,
        frame_mode: FrameComputationMode = FrameComputationMode.PRESERVE,
        resolver: _rust.MappingClassResolver | None = None,
        debug_info: DebugInfoPolicy | str = DebugInfoPolicy.PRESERVE,
        skip_debug: bool = False,
    ) -> Path:
        """Write the current archive state back to disk.

        The rewrite always flows through the native Rust archive layer. Supported
        transforms include Rust-backed transform/pipeline objects, their bound
        ``.apply`` methods, and plain Python callables that mutate ``ClassModel``
        in place.

        Signed-JAR artifacts under ``META-INF`` are preserved as ordinary
        resources and are **not** re-signed; if class bytes change the
        resulting archive may no longer verify.

        Args:
            output_path: Destination path.  When ``None`` the original file
                is overwritten.
            transform: Optional Rust-backed transform or pipeline, a bound
                ``.apply`` method from a Rust pipeline object, or a plain
                Python callable that mutates ``ClassModel`` in place.
            frame_mode: Frame policy to use when lowering classes.
            resolver: Class hierarchy resolver used during frame computation.
            debug_info: Policy controlling how debug attributes are emitted.
            skip_debug: If ``True``, strip debug attributes during rewrite.

        Returns:
            The resolved destination ``Path``.

        Raises:
            TypeError: If *transform* is not a supported Rust-backed transform
                or callable, if a Python transform returns a non-``None``
                value, or if *resolver* is not a Rust mapping resolver.
        """

        debug_policy = normalize_debug_info_policy(debug_info)
        if transform is not None and not _is_supported_transform(transform):
            raise TypeError(
                "JarFile.rewrite() requires a callable transform, ClassTransform, "
                "Pipeline, CompiledPipeline, or bound .apply method"
            )
        destination = Path(self.filename if output_path is None else output_path)
        destination.parent.mkdir(parents=True, exist_ok=True)
        original_filename = self.filename
        original_infolist = self.infolist.copy()
        original_files = self.files.copy()
        original_entry_states = self._entry_states.copy()
        effective_debug_info = _effective_rust_debug_policy(debug_policy, skip_debug=skip_debug)
        try:
            _rust.rewrite_archive_state(
                self.filename,
                list(self._entry_states.values()),
                transform=transform,
                output_path=destination,
                frame_mode=frame_mode,
                resolver=resolver,
                debug_info=effective_debug_info,
            )
            new_infolist, new_files, new_entry_states = _read_archive_state(destination)
            self.filename = os.fspath(destination)
            self.infolist = new_infolist
            self.files = new_files
            self._entry_states = new_entry_states
            return destination
        except Exception:
            self.filename = original_filename
            self.infolist = original_infolist
            self.files = original_files
            self._entry_states = original_entry_states
            raise
