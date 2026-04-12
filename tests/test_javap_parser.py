"""Unit tests for the javap output parser and semantic diff engine."""

from __future__ import annotations

from tests.javap_parser import (
    DiffSeverity,
    JavapClass,
    JavapCodeBlock,
    JavapCPEntry,
    JavapField,
    JavapInstruction,
    JavapMethod,
    _extract_field_name,
    _extract_method_name,
    _resolve_operand,
    _split_comment,
    parse_javap,
    semantic_diff,
)

# ---------------------------------------------------------------------------
# _split_comment
# ---------------------------------------------------------------------------


class TestSplitComment:
    def test_no_comment(self) -> None:
        value, comment = _split_comment("#7")
        assert value == "#7"
        assert comment == ""

    def test_with_comment(self) -> None:
        value, comment = _split_comment("#7                  // Field java/lang/System.out:Ljava/io/PrintStream;")
        assert value == "#7"
        assert comment == "Field java/lang/System.out:Ljava/io/PrintStream;"

    def test_double_slash_in_value(self) -> None:
        # Only the first // should be treated as comment separator
        value, comment = _split_comment("some//thing // actual comment")
        assert value == "some"
        assert comment == "thing // actual comment"


# ---------------------------------------------------------------------------
# _extract_method_name / _extract_field_name
# ---------------------------------------------------------------------------


class TestExtractMemberName:
    def test_constructor(self) -> None:
        assert _extract_method_name("public HelloWorld();") == "HelloWorld"

    def test_static_method(self) -> None:
        assert _extract_method_name("public static void main(java.lang.String[]);") == "main"

    def test_private_field(self) -> None:
        assert _extract_field_name("private final int count;") == "count"

    def test_generic_return(self) -> None:
        assert _extract_method_name("public java.util.List<java.lang.String> getItems();") == "getItems"

    def test_default_access_method(self) -> None:
        assert _extract_method_name("void doStuff();") == "doStuff"

    def test_array_field(self) -> None:
        assert _extract_field_name("private byte[] data;") == "data"


# ---------------------------------------------------------------------------
# parse_javap — minimal output
# ---------------------------------------------------------------------------

_MINIMAL_JAVAP = """\
Classfile /tmp/HelloWorld.class
  Last modified Jan 1, 2025; size 400 bytes
  SHA-256 checksum deadbeef
  Compiled from "HelloWorld.java"
public class HelloWorld
  minor version: 0
  major version: 52
  flags: (0x0021) ACC_PUBLIC, ACC_SUPER
  this_class: #5                          // HelloWorld
  super_class: #2                         // java/lang/Object
  interfaces: 0, fields: 1, methods: 2, attributes: 1
Constant pool:
   #1 = Methodref          #2.#3          // java/lang/Object."<init>":()V
   #2 = Class              #4             // java/lang/Object
   #3 = NameAndType        #10:#11        // "<init>":()V
   #4 = Utf8               java/lang/Object
   #5 = Class              #6             // HelloWorld
   #6 = Utf8               HelloWorld
   #7 = Utf8               Code
   #8 = Utf8               value
   #9 = Utf8               I
  #10 = Utf8               <init>
  #11 = Utf8               ()V
  #12 = Utf8               main
  #13 = Utf8               ([Ljava/lang/String;)V
  #14 = Utf8               SourceFile
  #15 = Utf8               HelloWorld.java
{
  private int value;
    descriptor: I
    flags: (0x0002) ACC_PRIVATE

  public HelloWorld();
    descriptor: ()V
    flags: (0x0001) ACC_PUBLIC
    Code:
      stack=1, locals=1, args_size=1
         0: aload_0
         1: invokespecial #1                  // Method java/lang/Object."<init>":()V
         4: return
      LineNumberTable:
        line 1: 0

  public static void main(java.lang.String[]);
    descriptor: ([Ljava/lang/String;)V
    flags: (0x0009) ACC_PUBLIC, ACC_STATIC
    Code:
      stack=0, locals=1, args_size=1
         0: return
      LineNumberTable:
        line 3: 0
}
SourceFile: "HelloWorld.java"
"""


class TestParseJavapMinimal:
    def test_class_header(self) -> None:
        cls = parse_javap(_MINIMAL_JAVAP)
        assert cls.major_version == 52
        assert cls.minor_version == 0
        assert cls.flags == "(0x0021) ACC_PUBLIC, ACC_SUPER"
        assert cls.this_class == "HelloWorld"
        assert cls.super_class == "java/lang/Object"
        assert cls.interfaces == []

    def test_constant_pool(self) -> None:
        cls = parse_javap(_MINIMAL_JAVAP)
        assert len(cls.constant_pool) == 15
        # CP entries are stored as a list; access by list index (0-based for entry #1)
        assert cls.constant_pool[0].tag == "Methodref"
        assert cls.constant_pool[1].tag == "Class"
        assert cls.constant_pool[3].tag == "Utf8"
        assert cls.constant_pool[3].value == "java/lang/Object"

    def test_fields(self) -> None:
        cls = parse_javap(_MINIMAL_JAVAP)
        assert len(cls.fields) == 1
        f = cls.fields[0]
        assert f.name == "value"
        assert f.descriptor == "I"
        assert "(0x0002) ACC_PRIVATE" in f.flags

    def test_methods(self) -> None:
        cls = parse_javap(_MINIMAL_JAVAP)
        assert len(cls.methods) == 2

        init = cls.methods[0]
        assert init.name == "HelloWorld"
        assert init.descriptor == "()V"
        assert "ACC_PUBLIC" in init.flags
        assert init.code is not None
        assert len(init.code.instructions) == 3
        assert init.code.instructions[0].mnemonic == "aload_0"
        assert init.code.instructions[1].mnemonic == "invokespecial"
        assert init.code.instructions[2].mnemonic == "return"

        main = cls.methods[1]
        assert main.name == "main"
        assert main.descriptor == "([Ljava/lang/String;)V"
        assert "ACC_PUBLIC" in main.flags
        assert "ACC_STATIC" in main.flags
        assert main.code is not None
        assert len(main.code.instructions) == 1
        assert main.code.instructions[0].mnemonic == "return"

    def test_code_metadata(self) -> None:
        cls = parse_javap(_MINIMAL_JAVAP)
        init = cls.methods[0]
        assert init.code is not None
        assert init.code.max_stack == 1
        assert init.code.max_locals == 1


# ---------------------------------------------------------------------------
# parse_javap — edge cases
# ---------------------------------------------------------------------------


class TestParseJavapEdgeCases:
    def test_empty_string(self) -> None:
        cls = parse_javap("")
        assert cls.major_version == 0
        assert cls.methods == []
        assert cls.fields == []
        assert cls.constant_pool == []

    def test_header_only(self) -> None:
        text = """\
public class Foo
  minor version: 3
  major version: 65
  flags: (0x0021) ACC_PUBLIC, ACC_SUPER
  this_class: #1                          // Foo
  super_class: #2                         // java/lang/Object
  interfaces: 0, fields: 0, methods: 0, attributes: 0
Constant pool:
   #1 = Class              #3             // Foo
   #2 = Class              #4             // java/lang/Object
   #3 = Utf8               Foo
   #4 = Utf8               java/lang/Object
{
}
"""
        cls = parse_javap(text)
        assert cls.major_version == 65
        assert cls.minor_version == 3
        assert cls.this_class == "Foo"
        assert cls.methods == []
        assert cls.fields == []
        assert len(cls.constant_pool) == 4

    def test_deprecated_method(self) -> None:
        text = """\
public class Dep
  minor version: 0
  major version: 52
  flags: (0x0021) ACC_PUBLIC, ACC_SUPER
  this_class: #1                          // Dep
  super_class: #2                         // java/lang/Object
  interfaces: 0, fields: 0, methods: 1, attributes: 0
Constant pool:
   #1 = Class              #3             // Dep
   #2 = Class              #4             // java/lang/Object
   #3 = Utf8               Dep
   #4 = Utf8               java/lang/Object
{
  public void old();
    descriptor: ()V
    flags: (0x0001) ACC_PUBLIC
    Code:
      stack=0, locals=1, args_size=1
         0: return
      LineNumberTable:
        line 3: 0
    Deprecated: true
}
"""
        cls = parse_javap(text)
        assert len(cls.methods) == 1
        # Deprecated is handled as a separate javap attribute line; the parser
        # captures the method itself (name, descriptor, flags, code).
        assert cls.methods[0].name == "old"


# ---------------------------------------------------------------------------
# _resolve_operand
# ---------------------------------------------------------------------------


class TestResolveOperand:
    def test_no_cp_ref(self) -> None:
        assert _resolve_operand("42", "", {}) == "42"

    def test_resolved_from_cp_map(self) -> None:
        cp_map: dict[int, str] = {5: "hello"}
        result = _resolve_operand("#5", "", cp_map)
        assert result == "hello"

    def test_unresolved_cp_ref(self) -> None:
        result = _resolve_operand("#99", "", {})
        assert result == "#99"


# ---------------------------------------------------------------------------
# semantic_diff
# ---------------------------------------------------------------------------


def _empty_class(**kwargs: object) -> JavapClass:
    """Build a JavapClass with sensible defaults, overriding with *kwargs*."""
    defaults: dict[str, object] = {
        "major_version": 52,
        "minor_version": 0,
        "flags": "(0x0021) ACC_PUBLIC, ACC_SUPER",
        "this_class": "Foo",
        "super_class": "java/lang/Object",
        "interfaces": [],
        "constant_pool": [],
        "fields": [],
        "methods": [],
    }
    defaults.update(kwargs)
    return JavapClass(
        major_version=int(str(defaults["major_version"])),
        minor_version=int(str(defaults["minor_version"])),
        flags=str(defaults["flags"]),
        this_class=str(defaults["this_class"]),
        super_class=str(defaults["super_class"]),
        interfaces=list(defaults["interfaces"]),  # type: ignore[arg-type]
        constant_pool=list(defaults["constant_pool"]),  # type: ignore[arg-type]
        fields=list(defaults["fields"]),  # type: ignore[arg-type]
        methods=list(defaults["methods"]),  # type: ignore[arg-type]
    )


def _code(instrs: list[JavapInstruction]) -> JavapCodeBlock:
    """Shortcut to build a JavapCodeBlock with only instructions."""
    return JavapCodeBlock(max_stack=0, max_locals=0, instructions=instrs, exception_table=[])


class TestSemanticDiff:
    def test_identical_classes(self) -> None:
        cls = _empty_class()
        assert semantic_diff(cls, cls) == []

    def test_version_mismatch(self) -> None:
        gold = _empty_class(major_version=52)
        ours = _empty_class(major_version=61)
        diffs = semantic_diff(gold, ours)
        assert len(diffs) == 1
        assert diffs[0].severity is DiffSeverity.ERROR
        assert "version" in diffs[0].message

    def test_missing_method(self) -> None:
        gold = _empty_class(
            methods=[JavapMethod(name="foo", descriptor="()V", flags="", code=None)],
        )
        ours = _empty_class()
        diffs = semantic_diff(gold, ours)
        errors = [d for d in diffs if d.severity is DiffSeverity.ERROR]
        assert any("missing" in d.message for d in errors)

    def test_extra_method(self) -> None:
        gold = _empty_class()
        ours = _empty_class(
            methods=[JavapMethod(name="bar", descriptor="()V", flags="", code=None)],
        )
        diffs = semantic_diff(gold, ours)
        errors = [d for d in diffs if d.severity is DiffSeverity.ERROR]
        assert any("extra" in d.message for d in errors)

    def test_instruction_count_mismatch(self) -> None:
        gold = _empty_class(
            methods=[
                JavapMethod(
                    name="m",
                    descriptor="()V",
                    flags="",
                    code=_code(
                        [
                            JavapInstruction(0, "aload_0", "", ""),
                            JavapInstruction(1, "return", "", ""),
                        ]
                    ),
                ),
            ],
        )
        ours = _empty_class(
            methods=[
                JavapMethod(
                    name="m",
                    descriptor="()V",
                    flags="",
                    code=_code([JavapInstruction(0, "return", "", "")]),
                ),
            ],
        )
        diffs = semantic_diff(gold, ours)
        errors = [d for d in diffs if d.severity is DiffSeverity.ERROR]
        assert any("instruction count" in d.message for d in errors)

    def test_field_flags_mismatch(self) -> None:
        gold = _empty_class(
            fields=[JavapField(name="x", descriptor="I", flags="ACC_PRIVATE")],
        )
        ours = _empty_class(
            fields=[JavapField(name="x", descriptor="I", flags="ACC_PUBLIC")],
        )
        diffs = semantic_diff(gold, ours)
        errors = [d for d in diffs if d.severity is DiffSeverity.ERROR]
        assert len(errors) == 1
        assert "flags" in errors[0].message

    def test_missing_field(self) -> None:
        gold = _empty_class(
            fields=[JavapField(name="x", descriptor="I", flags="ACC_PRIVATE")],
        )
        ours = _empty_class()
        diffs = semantic_diff(gold, ours)
        errors = [d for d in diffs if d.severity is DiffSeverity.ERROR]
        assert any("missing" in d.message for d in errors)

    def test_cp_index_only_diff_is_info(self) -> None:
        gold = _empty_class(
            constant_pool=[JavapCPEntry(1, "Methodref", "#2.#3", 'java/lang/Object."<init>":()V')],
            methods=[
                JavapMethod(
                    name="m",
                    descriptor="()V",
                    flags="",
                    code=_code([JavapInstruction(0, "invokespecial", "#1", 'Method java/lang/Object."<init>":()V')]),
                ),
            ],
        )
        ours = _empty_class(
            constant_pool=[JavapCPEntry(99, "Methodref", "#X.#Y", 'java/lang/Object."<init>":()V')],
            methods=[
                JavapMethod(
                    name="m",
                    descriptor="()V",
                    flags="",
                    code=_code([JavapInstruction(0, "invokespecial", "#99", 'Method java/lang/Object."<init>":()V')]),
                ),
            ],
        )
        diffs = semantic_diff(gold, ours)
        info = [d for d in diffs if d.severity is DiffSeverity.INFO]
        assert any("cp-ordering" == d.category for d in info)
