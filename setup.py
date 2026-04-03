"""Build script for optional Cython extensions.

When Cython is installed, all ``.pyx`` files under ``pytecode/`` are
compiled into C extension modules.  When Cython is absent, the build
falls through to a pure-Python package with no compiled extensions.
"""

from __future__ import annotations

from setuptools import setup

try:
    from Cython.Build import cythonize

    ext_modules = cythonize(
        "pytecode/**/*.pyx",
        language_level="3",
        compiler_directives={
            "boundscheck": False,
            "wraparound": False,
            "cdivision": True,
        },
    )
except ImportError:
    ext_modules = []

setup(ext_modules=ext_modules)
