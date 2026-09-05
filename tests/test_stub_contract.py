"""Keep the shipped native stub surface aligned with runtime capabilities."""

import ast
import inspect
from pathlib import Path

from pytecode import _rust
from pytecode.classfile import ClassReader
from pytecode.model import ClassModel, FieldModel, MethodModel
from tests.helpers import minimal_classfile


def test_declared_native_members_exist_at_runtime():
    stub = Path(_rust.__file__).with_name("_rust.pyi")
    tree = ast.parse(stub.read_text(encoding="utf-8"))
    missing: list[str] = []
    for declaration in tree.body:
        if not isinstance(declaration, ast.ClassDef) or declaration.name.startswith("_"):
            continue
        if any(isinstance(base, ast.Name) and base.id == "TypedDict" for base in declaration.bases):
            continue  # Dictionary shapes are static typing declarations, not extension classes.
        runtime = getattr(_rust, declaration.name)
        for member in declaration.body:
            if isinstance(member, ast.FunctionDef) and not member.name.startswith("_"):
                try:
                    inspect.getattr_static(runtime, member.name)
                except AttributeError:
                    missing.append(f"{declaration.name}.{member.name}")
    assert not missing, missing


def test_model_setters_and_read_only_instruction_view(tmp_path: Path):
    data = minimal_classfile()
    model = ClassModel.from_bytes(data)
    model.name = "Renamed"
    model.version = (52, 0)
    method = MethodModel("run", "()V", 0x0009)
    method.set_raw_code(0, 0, b"\xb1")
    model.methods = [method]
    model.fields = [FieldModel("value", "I", 0x0001)]
    model.fields[0].name = "renamedField"
    model.methods[0].name = "renamedMethod"
    code = model.methods[0].code
    assert code is not None
    assert not hasattr(code.instructions, "append")
    assert not hasattr(code.instructions, "clear")
    result = ClassModel.from_bytes(model.to_bytes())
    assert result.name == "Renamed" and result.version == (52, 0)
    assert result.fields[0].name == "renamedField"
    assert result.methods[0].name == "renamedMethod"
    path = tmp_path / "Renamed.class"
    path.write_bytes(result.to_bytes())
    assert ClassReader.from_file(path).class_info.major_version == 52
