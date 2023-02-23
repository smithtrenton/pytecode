import os
import zipfile
from dataclasses import dataclass


@dataclass
class JarInfo:
    filename: str
    zipinfo: zipfile.ZipInfo
    bytes: bytes


class JarFile:
    def __init__(self, filename):
        self.filename = filename
        self.read()

    def read(self):
        with zipfile.ZipFile(self.filename, 'r') as jar:
            self.infolist = jar.infolist()
            self.files = {}
            for info in self.infolist:
                fn = os.path.join(*info.filename.split('/'))
                self.files[fn] = JarInfo(fn, info, jar.read(info.filename))
