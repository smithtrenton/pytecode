"""Tests for the API documentation validation logic."""

from __future__ import annotations

import importlib

from tools.generate_api_docs import (
    PUBLIC_MODULES,
    get_public_symbols,
    validate_docstrings,
)


class TestPublicSurface:
    """Verify the public-surface manifest is consistent."""

    def test_all_modules_importable(self) -> None:
        for module_name in PUBLIC_MODULES:
            mod = importlib.import_module(module_name)
            assert mod is not None, f"{module_name} failed to import"

    def test_all_modules_have_dunder_all(self) -> None:
        for module_name in PUBLIC_MODULES:
            mod = importlib.import_module(module_name)
            assert hasattr(mod, "__all__"), f"{module_name} missing __all__"

    def test_dunder_all_entries_exist(self) -> None:
        for module_name in PUBLIC_MODULES:
            mod = importlib.import_module(module_name)
            for name in get_public_symbols(module_name):
                assert hasattr(mod, name), f"{module_name}.__all__ lists '{name}' but it doesn't exist"

    def test_no_private_modules(self) -> None:
        for module_name in PUBLIC_MODULES:
            parts = module_name.split(".")
            for part in parts:
                assert not part.startswith("_") or part == "__init__", (
                    f"Private module in PUBLIC_MODULES: {module_name}"
                )


class TestDocstringCoverage:
    """Verify every public symbol has a docstring."""

    def test_full_coverage(self) -> None:
        total, documented, missing = validate_docstrings()
        assert total > 0, "No public symbols found"
        assert not missing, f"{len(missing)} undocumented symbol(s):\n" + "\n".join(f"  - {s}" for s in sorted(missing))
        assert documented == total

    def test_module_docstrings(self) -> None:
        import inspect

        for module_name in PUBLIC_MODULES:
            mod = importlib.import_module(module_name)
            assert inspect.getdoc(mod), f"{module_name} missing module docstring"


class TestEdgeCases:
    """Verify edge cases for specific symbol types."""

    def test_enum_docstrings(self) -> None:
        import inspect

        from pytecode.classfile.constants import ClassAccessFlag, VerificationType

        assert inspect.getdoc(ClassAccessFlag)
        assert inspect.getdoc(VerificationType)

    def test_archive_surface_docstrings(self) -> None:
        import inspect
        from typing import cast

        from pytecode.archive import JarFile, JarInfo

        assert inspect.getdoc(cast(object, JarInfo))
        assert inspect.getdoc(cast(object, JarFile))

    def test_rust_transform_docstrings(self) -> None:
        import inspect

        from pytecode.transforms import PipelineBuilder, add_access_flags, class_named

        assert inspect.getdoc(PipelineBuilder)
        assert inspect.getdoc(class_named)
        assert inspect.getdoc(add_access_flags)

    def test_verifier_docstrings(self) -> None:
        import inspect

        from pytecode.analysis.verify import verify_classfile, verify_classmodel

        assert inspect.getdoc(verify_classfile)
        assert inspect.getdoc(verify_classmodel)

    def test_descriptor_docstrings(self) -> None:
        import inspect
        from typing import cast

        from pytecode.classfile.modified_utf8 import decode_modified_utf8, encode_modified_utf8

        assert inspect.getdoc(cast(object, decode_modified_utf8))
        assert inspect.getdoc(cast(object, encode_modified_utf8))

    def test_constant_docstring(self) -> None:
        import inspect

        from pytecode.classfile.constants import MAGIC

        assert inspect.getdoc(MAGIC)
