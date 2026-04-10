"""Read, modify, and rewrite JAR archives with optional bytecode transformation."""

from __future__ import annotations

import copy
import os
import tempfile
import zipfile
from collections.abc import Callable
from dataclasses import dataclass
from enum import Enum
from pathlib import Path, PurePosixPath

from .. import _rust

__all__ = ["JarFile", "JarInfo"]


class DebugInfoPolicy(Enum):
    """Policy controlling whether archive rewrites preserve or strip debug info."""

    PRESERVE = "preserve"
    STRIP = "strip"


_RustRewriteTransform = _rust.RustPipeline | _rust.RustCompiledPipeline
_RustTransformInput = _RustRewriteTransform | _rust.RustClassTransform
_RustBoundTransform = Callable[[_rust.RustClassModel], object | None]


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


def _normalize_rust_transform(
    transform: _RustTransformInput | _RustBoundTransform | None,
) -> _RustRewriteTransform | None:
    if transform is None:
        return None
    if isinstance(transform, (_rust.RustPipeline, _rust.RustCompiledPipeline)):
        return transform
    if isinstance(transform, _rust.RustClassTransform):
        pipeline = _rust.RustPipeline()
        pipeline.on_classes(_rust.RustClassMatcher.any(), transform)
        return pipeline
    owner = getattr(transform, "__self__", None)
    if isinstance(owner, (_rust.RustPipeline, _rust.RustCompiledPipeline)):
        return owner
    return None


def _apply_rust_transform(transform: _RustRewriteTransform, model: _rust.RustClassModel) -> None:
    transform.apply(model)


def _effective_rust_debug_policy(debug_policy: DebugInfoPolicy, *, skip_debug: bool) -> str:
    return DebugInfoPolicy.STRIP.value if skip_debug else debug_policy.value


def normalize_debug_info_policy(policy: DebugInfoPolicy | str) -> DebugInfoPolicy:
    """Normalize archive debug-info policy values."""

    if isinstance(policy, DebugInfoPolicy):
        return policy
    try:
        return DebugInfoPolicy(policy)
    except ValueError as exc:
        raise ValueError("debug_info must be one of: preserve, strip") from exc


def _read_archive_state(filename: str | os.PathLike[str]) -> tuple[list[zipfile.ZipInfo], dict[str, JarInfo]]:
    files: dict[str, JarInfo] = {}
    with zipfile.ZipFile(filename, "r") as jar:
        infolist = jar.infolist()
        for info in infolist:
            normalized = _normalize_filename(info.filename, is_dir=info.is_dir())
            data = b"" if info.is_dir() else jar.read(info.filename)
            files[normalized] = JarInfo(normalized, info, data)
    return infolist, files


def _zipinfo_metadata_signature(info: zipfile.ZipInfo) -> tuple[object, ...]:
    return (
        info.filename,
        info.comment,
        info.extra,
        info.compress_type,
        info.date_time,
        info.external_attr,
        info.flag_bits,
        info.create_system,
        info.create_version,
        info.extract_version,
        info.volume,
        info.internal_attr,
        info.is_dir(),
    )


def _archive_state_matches_disk(
    filename: str | os.PathLike[str],
    files: dict[str, JarInfo],
) -> bool:
    _, disk_files = _read_archive_state(filename)
    if list(files) != list(disk_files):
        return False
    for entry_name, jar_info in files.items():
        disk_info = disk_files.get(entry_name)
        if disk_info is None:
            return False
        if jar_info.bytes != disk_info.bytes:
            return False
        if _zipinfo_metadata_signature(jar_info.zipinfo) != _zipinfo_metadata_signature(disk_info.zipinfo):
            return False
    return True


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

    def parse_classes(self) -> tuple[list[tuple[JarInfo, _rust.ClassReader]], list[JarInfo]]:
        """Parse all ``.class`` entries into Rust-backed readers and separate them from other resources.

        Returns:
            A two-element tuple of (class entries, non-class entries).
            Each class entry is a ``(JarInfo, pytecode.ClassReader)`` pair.
        """
        classes: list[tuple[JarInfo, _rust.ClassReader]] = []
        other_files: list[JarInfo] = []
        for jar_info in self.files.values():
            if _is_class_filename(jar_info.filename):
                classes.append((jar_info, _rust.ClassReader.from_bytes(jar_info.bytes)))
            else:
                other_files.append(jar_info)
        return classes, other_files

    def rewrite(
        self,
        output_path: str | os.PathLike[str] | None = None,
        *,
        transform: _RustTransformInput | _RustBoundTransform | None = None,
        recompute_frames: bool = False,
        resolver: _rust.RustMappingClassResolver | None = None,
        debug_info: DebugInfoPolicy | str = DebugInfoPolicy.PRESERVE,
        skip_debug: bool = False,
    ) -> Path:
        """Write the current archive state back to disk.

        By default the archive is rewritten in place. ``.class`` entries are
        copied verbatim unless *transform* or non-default rewrite options
        require Rust-backed reserialization.

        Signed-JAR artifacts under ``META-INF`` are preserved as ordinary
        resources and are **not** re-signed; if class bytes change the
        resulting archive may no longer verify.

        Args:
            output_path: Destination path.  When ``None`` the original file
                is overwritten.
            transform: Optional Rust-backed transform or pipeline (or a bound
                ``.apply`` method from a Rust pipeline object).
            recompute_frames: Whether to recompute ``StackMapTable`` frames
                when lowering classes.
            resolver: Class hierarchy resolver used during frame computation.
            debug_info: Policy controlling how debug attributes are emitted.
            skip_debug: If ``True``, strip debug attributes during rewrite.

        Returns:
            The resolved destination ``Path``.

        Raises:
            TypeError: If *transform* is not a supported Rust-backed transform
                or if *resolver* is not a Rust mapping resolver.
        """

        debug_policy = normalize_debug_info_policy(debug_info)
        rust_transform = _normalize_rust_transform(transform)
        if transform is not None and rust_transform is None:
            raise TypeError(
                "JarFile.rewrite() requires a RustClassTransform, RustPipeline, "
                "RustCompiledPipeline, or bound Rust .apply method"
            )
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
            if rust_transform is not None and not skip_debug and _archive_state_matches_disk(self.filename, self.files):
                try:
                    _rust.rewrite_archive_with_rust_transform(
                        self.filename,
                        rust_transform,
                        output_path=destination,
                        recompute_frames=recompute_frames,
                        resolver=resolver,
                        debug_info=debug_policy.value,
                    )
                except NotImplementedError:
                    pass
                else:
                    new_infolist, new_files = _read_archive_state(destination)
                    self.filename = os.fspath(destination)
                    self.infolist = new_infolist
                    self.files = new_files
                    return destination

            with zipfile.ZipFile(temp_path, "w") as jar:
                for jar_info in self.files.values():
                    data = jar_info.bytes
                    if should_rewrite_classes and _is_class_filename(jar_info.filename):
                        if rust_transform is not None:
                            if skip_debug:
                                raise ValueError(
                                    "JarFile.rewrite() skip_debug is not supported with Rust-backed transforms"
                                )
                            rust_model = _rust.RustClassModel.from_bytes(data)
                            _apply_rust_transform(rust_transform, rust_model)
                            data = bytes(
                                rust_model.to_bytes_with_options(
                                    recompute_frames=recompute_frames,
                                    resolver=resolver,
                                    debug_info=debug_policy.value,
                                )
                            )
                        else:
                            rust_model = _rust.RustClassModel.from_bytes(data)
                            data = bytes(
                                rust_model.to_bytes_with_options(
                                    recompute_frames=recompute_frames,
                                    resolver=resolver,
                                    debug_info=_effective_rust_debug_policy(
                                        debug_policy,
                                        skip_debug=skip_debug,
                                    ),
                                )
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
