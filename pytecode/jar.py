from __future__ import annotations

import os
import zipfile
from dataclasses import dataclass
from pathlib import Path, PurePosixPath

from .class_reader import ClassReader


@dataclass
class JarInfo:
    filename: str
    zipinfo: zipfile.ZipInfo
    bytes: bytes


class JarFile:
    def __init__(self, filename: str | os.PathLike[str]) -> None:
        self.filename = os.fspath(filename)
        self.infolist: list[zipfile.ZipInfo] = []
        self.files: dict[str, JarInfo] = {}
        self.read()

    def read(self) -> None:
        with zipfile.ZipFile(self.filename, "r") as jar:
            self.infolist = jar.infolist()
            for info in self.infolist:
                filename = str(Path(*PurePosixPath(info.filename).parts))
                self.files[filename] = JarInfo(filename, info, jar.read(info.filename))

    def parse_classes(self) -> tuple[list[tuple[JarInfo, ClassReader]], list[JarInfo]]:
        classes: list[tuple[JarInfo, ClassReader]] = []
        other_files: list[JarInfo] = []
        for fn, jar_info in self.files.items():
            if fn.endswith(".class"):
                classes.append((jar_info, ClassReader.from_bytes(jar_info.bytes)))
            else:
                other_files.append(jar_info)
        return classes, other_files
