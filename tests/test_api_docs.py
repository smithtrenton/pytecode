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

    def test_bytes_utils_excluded(self) -> None:
        assert "pytecode.bytes_utils" not in PUBLIC_MODULES

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

        from pytecode.constants import ClassAccessFlag, VerificationType

        assert inspect.getdoc(ClassAccessFlag)
        assert inspect.getdoc(VerificationType)

    def test_dataclass_docstrings(self) -> None:
        import inspect

        from pytecode.info import ClassFile, FieldInfo, MethodInfo

        assert inspect.getdoc(ClassFile)
        assert inspect.getdoc(FieldInfo)
        assert inspect.getdoc(MethodInfo)

    def test_protocol_docstrings(self) -> None:
        import inspect

        from pytecode.transforms import ClassTransform, CodeTransform

        assert inspect.getdoc(ClassTransform)
        assert inspect.getdoc(CodeTransform)

    def test_overloaded_function_docstrings(self) -> None:
        import inspect

        from pytecode.debug_info import (
            apply_debug_info_policy,
            mark_code_debug_info_stale,
            strip_debug_info,
        )

        assert inspect.getdoc(mark_code_debug_info_stale)
        assert inspect.getdoc(apply_debug_info_policy)
        assert inspect.getdoc(strip_debug_info)

    def test_type_alias_docstrings(self) -> None:
        import inspect

        from pytecode.operands import LdcValue

        assert inspect.getdoc(LdcValue)

    def test_constant_docstring(self) -> None:
        import inspect

        from pytecode.constants import MAGIC

        assert inspect.getdoc(MAGIC)
