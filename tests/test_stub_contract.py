"""Keep the shipped native stub surface aligned with runtime capabilities."""

import ast
import inspect
from pathlib import Path

from pytecode import _rust
from pytecode.classfile import ClassReader
from pytecode.model import ClassModel, FieldModel, MethodModel
from tests.helpers import minimal_classfile

_CHECKED_DUNDERS = ("__len__", "__getitem__", "__iter__", "__and__", "__or__", "__invert__", "__eq__", "__hash__")


def _stub_classes_and_functions(tree: ast.Module) -> tuple[dict[str, ast.ClassDef], dict[str, ast.FunctionDef]]:
    classes: dict[str, ast.ClassDef] = {}
    functions: dict[str, ast.FunctionDef] = {}
    for node in tree.body:
        if isinstance(node, ast.ClassDef):
            is_typed_dict = any(isinstance(base, ast.Name) and base.id == "TypedDict" for base in node.bases)
            if not is_typed_dict:
                classes[node.name] = node
        elif isinstance(node, ast.FunctionDef):
            functions[node.name] = node
    return classes, functions


def _stub_class_members(node: ast.ClassDef) -> set[str]:
    members: set[str] = set()
    for member in node.body:
        if isinstance(member, ast.FunctionDef):
            if not member.name.startswith("_") or member.name in _CHECKED_DUNDERS:
                members.add(member.name)
    return members


def test_declared_native_members_exist_at_runtime():
    stub = Path(_rust.__file__).with_name("_rust.pyi")
    tree = ast.parse(stub.read_text(encoding="utf-8"))
    stub_classes, stub_functions = _stub_classes_and_functions(tree)

    missing_stub_to_runtime: list[str] = []
    for class_name, node in stub_classes.items():
        if not hasattr(_rust, class_name):
            missing_stub_to_runtime.append(class_name)
            continue
        runtime = getattr(_rust, class_name)
        for member_name in sorted(_stub_class_members(node)):
            try:
                inspect.getattr_static(runtime, member_name)
            except AttributeError:
                missing_stub_to_runtime.append(f"{class_name}.{member_name}")
    for func_name in stub_functions:
        if not hasattr(_rust, func_name):
            missing_stub_to_runtime.append(func_name)
    assert not missing_stub_to_runtime, missing_stub_to_runtime

    missing_runtime_to_stub: list[str] = []
    for name in sorted(dir(_rust)):
        if name.startswith("__"):
            continue
        obj = getattr(_rust, name)
        if inspect.isclass(obj):
            if name not in stub_classes:
                missing_runtime_to_stub.append(name)
                continue
            declared = _stub_class_members(stub_classes[name])
            for member_name in sorted(obj.__dict__):
                if not member_name.startswith("_") and member_name not in declared:
                    missing_runtime_to_stub.append(f"{name}.{member_name}")
        elif callable(obj) and name not in stub_functions:
            missing_runtime_to_stub.append(name)
    assert not missing_runtime_to_stub, missing_runtime_to_stub


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
