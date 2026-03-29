"""Structured parser for ``javap -v -p -c`` output and CP-aware semantic diff engine.

Provides dataclasses modelling every section of verbose javap output (constant pool,
fields, methods, instructions, exception tables), a :func:`parse_javap` function that
turns the raw text into a :class:`JavapClass` tree, and a :func:`semantic_diff`
function that compares two parsed trees with constant-pool-aware resolution.
"""

from __future__ import annotations

import re
import sys
from dataclasses import dataclass
from enum import Enum

# ---------------------------------------------------------------------------
# Dataclasses
# ---------------------------------------------------------------------------


@dataclass
class JavapCPEntry:
    """A single constant-pool entry parsed from javap output."""

    index: int
    tag: str
    value: str
    comment: str


@dataclass
class JavapInstruction:
    """A single bytecode instruction."""

    offset: int
    mnemonic: str
    operands: str
    comment: str


@dataclass
class JavapExceptionEntry:
    """One row of a Code attribute's exception table."""

    from_pc: int
    to_pc: int
    target: int
    type: str


@dataclass
class JavapCodeBlock:
    """Parsed Code attribute of a method."""

    max_stack: int
    max_locals: int
    instructions: list[JavapInstruction]
    exception_table: list[JavapExceptionEntry]


@dataclass
class JavapMethod:
    """A method declaration with optional Code block."""

    name: str
    descriptor: str
    flags: str
    code: JavapCodeBlock | None


@dataclass
class JavapField:
    """A field declaration."""

    name: str
    descriptor: str
    flags: str


@dataclass
class JavapClass:
    """Top-level result of parsing javap output."""

    major_version: int
    minor_version: int
    flags: str
    this_class: str
    super_class: str
    interfaces: list[str]
    constant_pool: list[JavapCPEntry]
    fields: list[JavapField]
    methods: list[JavapMethod]


# ---------------------------------------------------------------------------
# Compiled regex patterns
# ---------------------------------------------------------------------------

_CP_LINE_RE = re.compile(r"^\s+#(\d+)\s*=\s*(\w+)\s+(.*)")
_INSTR_LINE_RE = re.compile(r"^\s+(\d+):\s+(\S+)(?:\s+(.*))?$")
_CODE_HDR_RE = re.compile(r"stack=(\d+),\s*locals=(\d+)")
_EXC_ROW_RE = re.compile(r"^\s+(\d+)\s+(\d+)\s+(\d+)\s+(.+)$")


# ---------------------------------------------------------------------------
# Regex helper
# ---------------------------------------------------------------------------


def _group(m: re.Match[str], n: int) -> str:
    """Extract regex group *n*, returning ``""`` for non-participating groups."""
    v = m.group(n)
    return v if isinstance(v, str) else ""


# ---------------------------------------------------------------------------
# Parsing helpers
# ---------------------------------------------------------------------------


def _split_comment(text: str) -> tuple[str, str]:
    """Split *text* at the first ``//`` into ``(value, comment)``."""
    if "//" in text:
        value, _, comment = text.partition("//")
        return value.strip(), comment.strip()
    return text.strip(), ""


def _indent(line: str) -> int:
    """Return the number of leading spaces in *line*."""
    return len(line) - len(line.lstrip())


def _extract_method_name(declaration: str) -> str:
    """Return the method name from a javap method declaration."""
    if "(" in declaration:
        before = declaration[: declaration.index("(")]
        parts = before.split()
        return parts[-1] if parts else declaration
    # static initializer: ``static {};``
    if "{}" in declaration:
        return "<clinit>"
    parts = declaration.rstrip(";").split()
    return parts[-1] if parts else declaration


def _extract_field_name(declaration: str) -> str:
    """Return the field name from a javap field declaration."""
    cleaned = declaration.rstrip(";").strip()
    parts = cleaned.split()
    return parts[-1] if parts else cleaned


# ---------------------------------------------------------------------------
# Constant pool
# ---------------------------------------------------------------------------


def _parse_constant_pool(lines: list[str], start: int, end: int) -> list[JavapCPEntry]:
    """Parse constant-pool lines between *start* (inclusive) and *end* (exclusive)."""
    entries: list[JavapCPEntry] = []
    for i in range(start, end):
        m = _CP_LINE_RE.match(lines[i])
        if m is None:
            continue
        index = int(_group(m, 1))
        tag = _group(m, 2)
        rest = _group(m, 3)
        value, comment = _split_comment(rest)
        entries.append(JavapCPEntry(index=index, tag=tag, value=value, comment=comment))
    return entries


# ---------------------------------------------------------------------------
# Code block
# ---------------------------------------------------------------------------


def _parse_code_block(lines: list[str], start: int) -> tuple[JavapCodeBlock, int]:
    """Parse a ``Code:`` block starting at the line *after* the ``Code:`` header."""
    max_stack = 0
    max_locals = 0
    instructions: list[JavapInstruction] = []
    exception_table: list[JavapExceptionEntry] = []
    i = start

    # stack / locals header
    if i < len(lines):
        hdr = _CODE_HDR_RE.search(lines[i])
        if hdr is not None:
            max_stack = int(_group(hdr, 1))
            max_locals = int(_group(hdr, 2))
            i += 1

    while i < len(lines):
        line = lines[i]
        stripped = line.strip()
        if not stripped:
            i += 1
            continue

        ind = _indent(line)
        # Exited code block (back to attribute or member level)
        if ind <= 4:
            break

        # Instruction
        im = _INSTR_LINE_RE.match(line)
        if im is not None:
            offset = int(_group(im, 1))
            mnemonic = _group(im, 2)
            rest = _group(im, 3)
            operands, comment = _split_comment(rest)
            instructions.append(JavapInstruction(offset, mnemonic, operands, comment))
            i += 1
            continue

        # Exception table
        if "Exception table:" in stripped:
            i += 1
            if i < len(lines) and "from" in lines[i]:
                i += 1
            while i < len(lines):
                em = _EXC_ROW_RE.match(lines[i])
                if em is not None:
                    exception_table.append(
                        JavapExceptionEntry(
                            int(_group(em, 1)),
                            int(_group(em, 2)),
                            int(_group(em, 3)),
                            _group(em, 4).strip(),
                        )
                    )
                    i += 1
                else:
                    break
            continue

        # Skip sub-attributes (LineNumberTable, StackMapTable, etc.)
        i += 1

    return JavapCodeBlock(max_stack, max_locals, instructions, exception_table), i


# ---------------------------------------------------------------------------
# Members (fields & methods)
# ---------------------------------------------------------------------------


def _parse_members(
    lines: list[str],
    brace_start: int,
    brace_end: int,
) -> tuple[list[JavapField], list[JavapMethod]]:
    """Parse fields and methods from lines between ``{`` and ``}``."""
    fields: list[JavapField] = []
    methods: list[JavapMethod] = []
    i = brace_start + 1

    while i < brace_end:
        line = lines[i]
        stripped = line.strip()
        if not stripped:
            i += 1
            continue

        ind = _indent(line)
        # Member declarations live at 2-space indent and end with ';'
        if ind == 2 and stripped.endswith(";"):
            declaration = stripped
            descriptor = ""
            member_flags = ""
            code: JavapCodeBlock | None = None
            i += 1

            # Collect attributes belonging to this member
            while i < brace_end:
                attr_line = lines[i]
                attr_stripped = attr_line.strip()
                if not attr_stripped:
                    i += 1
                    continue

                attr_ind = _indent(attr_line)
                # Next member starts at indent <= 2
                if attr_ind <= 2 and attr_stripped.endswith(";"):
                    break

                if attr_stripped.startswith("descriptor:"):
                    descriptor = attr_stripped.split(":", 1)[1].strip()
                elif attr_stripped.startswith("flags:"):
                    member_flags = attr_stripped.split(":", 1)[1].strip()
                elif attr_stripped.startswith("Code:"):
                    i += 1
                    code, i = _parse_code_block(lines, i)
                    continue

                i += 1

            # Classify as method or field by descriptor
            if descriptor.startswith("("):
                name = _extract_method_name(declaration)
                methods.append(JavapMethod(name=name, descriptor=descriptor, flags=member_flags, code=code))
            else:
                name = _extract_field_name(declaration)
                fields.append(JavapField(name=name, descriptor=descriptor, flags=member_flags))
        else:
            i += 1

    return fields, methods


# ---------------------------------------------------------------------------
# Top-level parser
# ---------------------------------------------------------------------------


def parse_javap(output: str) -> JavapClass:
    """Parse the full text output of ``javap -v -p -c`` into a :class:`JavapClass`."""
    lines = output.splitlines()

    major = 0
    minor = 0
    class_flags = ""
    this_class = ""
    super_class = ""
    interfaces: list[str] = []

    cp_start = -1
    brace_start = -1
    brace_end = -1

    for idx, line in enumerate(lines):
        stripped = line.strip()

        if stripped.startswith("major version:"):
            major = int(stripped.split(":", 1)[1].strip())
        elif stripped.startswith("minor version:"):
            minor = int(stripped.split(":", 1)[1].strip())
        elif stripped.startswith("flags:") and _indent(line) == 2:
            class_flags = stripped.split(":", 1)[1].strip()
        elif stripped.startswith("this_class:") and "//" in stripped:
            this_class = stripped.split("//", 1)[1].strip()
        elif stripped.startswith("super_class:") and "//" in stripped:
            super_class = stripped.split("//", 1)[1].strip()
        elif "implements " in stripped and _indent(line) == 0:
            impl_part = stripped.split("implements", 1)[1].strip()
            interfaces = [iface.strip() for iface in impl_part.split(",")]
        elif stripped == "Constant pool:":
            cp_start = idx + 1
        elif stripped == "{" and brace_start < 0:
            brace_start = idx
        elif stripped == "}" and brace_start >= 0 and brace_end < 0:
            brace_end = idx

    constant_pool: list[JavapCPEntry] = []
    if cp_start >= 0 and brace_start >= 0:
        constant_pool = _parse_constant_pool(lines, cp_start, brace_start)

    fields_list: list[JavapField] = []
    methods_list: list[JavapMethod] = []
    if brace_start >= 0 and brace_end >= 0:
        fields_list, methods_list = _parse_members(lines, brace_start, brace_end)

    return JavapClass(
        major_version=major,
        minor_version=minor,
        flags=class_flags,
        this_class=this_class,
        super_class=super_class,
        interfaces=interfaces,
        constant_pool=constant_pool,
        fields=fields_list,
        methods=methods_list,
    )


# ---------------------------------------------------------------------------
# Semantic diff
# ---------------------------------------------------------------------------


class DiffSeverity(Enum):
    """Severity level for a semantic difference."""

    ERROR = "ERROR"
    WARNING = "WARNING"
    INFO = "INFO"


@dataclass
class SemanticDiff:
    """One semantic difference between two javap outputs."""

    severity: DiffSeverity
    category: str
    message: str


def _cp_comment_map(cp: list[JavapCPEntry]) -> dict[int, str]:
    """Build index -> resolved-comment map for a constant pool."""
    result: dict[int, str] = {}
    for entry in cp:
        result[entry.index] = entry.comment if entry.comment else entry.value
    return result


def _resolve_operand(operands: str, comment: str, cp_map: dict[int, str]) -> str:
    """Resolve CP-ref operands to their symbolic string via *cp_map*."""
    stripped = operands.strip()
    if stripped.startswith("#"):
        end = 1
        while end < len(stripped) and stripped[end].isdigit():
            end += 1
        if end > 1:
            idx = int(stripped[1:end])
            resolved = cp_map.get(idx, "")
            if resolved:
                return resolved
        if comment:
            return comment
    return operands


_EQUIVALENT_OPCODES: list[tuple[str, str, str]] = [
    ("iconst_m1", "bipush", "-1"),
    ("iconst_0", "bipush", "0"),
    ("iconst_1", "bipush", "1"),
    ("iconst_2", "bipush", "2"),
    ("iconst_3", "bipush", "3"),
    ("iconst_4", "bipush", "4"),
    ("iconst_5", "bipush", "5"),
    ("lconst_0", "ldc2_w", "0"),
    ("lconst_1", "ldc2_w", "1"),
    ("fconst_0", "ldc", "0.0"),
    ("fconst_1", "ldc", "1.0"),
    ("fconst_2", "ldc", "2.0"),
    ("dconst_0", "ldc2_w", "0.0"),
    ("dconst_1", "ldc2_w", "1.0"),
]


def _are_equivalent(a: JavapInstruction, b: JavapInstruction) -> bool:
    """Return *True* when *a* and *b* are different but semantically equivalent opcodes."""
    for short, long, operand in _EQUIVALENT_OPCODES:
        if a.mnemonic == short and b.mnemonic == long and b.operands.strip() == operand:
            return True
        if b.mnemonic == short and a.mnemonic == long and a.operands.strip() == operand:
            return True
    return False


def _diff_instructions(
    gold_code: JavapCodeBlock,
    ours_code: JavapCodeBlock,
    gold_cp: dict[int, str],
    ours_cp: dict[int, str],
    method_key: str,
    diffs: list[SemanticDiff],
) -> None:
    """Compare instruction sequences for one method, appending to *diffs*."""
    g = gold_code.instructions
    o = ours_code.instructions
    if len(g) != len(o):
        diffs.append(
            SemanticDiff(
                DiffSeverity.ERROR,
                "instructions",
                f"{method_key}: instruction count differs ({len(g)} vs {len(o)})",
            )
        )
        return

    for pos, (gi, oi) in enumerate(zip(g, o)):
        if gi.mnemonic != oi.mnemonic:
            if _are_equivalent(gi, oi):
                diffs.append(
                    SemanticDiff(
                        DiffSeverity.WARNING,
                        "instruction-selection",
                        f"{method_key}[{pos}]: {gi.mnemonic} vs {oi.mnemonic} (equivalent)",
                    )
                )
            else:
                diffs.append(
                    SemanticDiff(
                        DiffSeverity.ERROR,
                        "instructions",
                        f"{method_key}[{pos}]: mnemonic {gi.mnemonic} vs {oi.mnemonic}",
                    )
                )
            continue

        # Mnemonics match -- compare operands semantically
        g_resolved = _resolve_operand(gi.operands, gi.comment, gold_cp)
        o_resolved = _resolve_operand(oi.operands, oi.comment, ours_cp)

        if g_resolved != o_resolved:
            diffs.append(
                SemanticDiff(
                    DiffSeverity.ERROR,
                    "instructions",
                    f"{method_key}[{pos}]: operand {gi.operands} ({g_resolved}) vs {oi.operands} ({o_resolved})",
                )
            )
        elif gi.operands != oi.operands:
            diffs.append(
                SemanticDiff(
                    DiffSeverity.INFO,
                    "cp-ordering",
                    f"{method_key}[{pos}]: CP index {gi.operands} vs {oi.operands} (same symbol)",
                )
            )


def semantic_diff(gold: JavapClass, ours: JavapClass) -> list[SemanticDiff]:
    """CP-aware semantic comparison of two parsed javap outputs."""
    diffs: list[SemanticDiff] = []

    # --- class structure ---
    if gold.major_version != ours.major_version or gold.minor_version != ours.minor_version:
        diffs.append(
            SemanticDiff(
                DiffSeverity.ERROR,
                "class-version",
                f"version {gold.major_version}.{gold.minor_version} vs {ours.major_version}.{ours.minor_version}",
            )
        )
    if gold.flags != ours.flags:
        diffs.append(SemanticDiff(DiffSeverity.ERROR, "class-flags", f"flags '{gold.flags}' vs '{ours.flags}'"))
    if gold.this_class != ours.this_class:
        diffs.append(
            SemanticDiff(
                DiffSeverity.ERROR,
                "class-name",
                f"this_class '{gold.this_class}' vs '{ours.this_class}'",
            )
        )
    if gold.super_class != ours.super_class:
        diffs.append(
            SemanticDiff(
                DiffSeverity.ERROR,
                "class-super",
                f"super_class '{gold.super_class}' vs '{ours.super_class}'",
            )
        )
    if gold.interfaces != ours.interfaces:
        diffs.append(
            SemanticDiff(
                DiffSeverity.ERROR,
                "class-interfaces",
                f"interfaces {gold.interfaces} vs {ours.interfaces}",
            )
        )

    # --- fields ---
    gold_fields: dict[tuple[str, str], JavapField] = {(f.name, f.descriptor): f for f in gold.fields}
    ours_fields: dict[tuple[str, str], JavapField] = {(f.name, f.descriptor): f for f in ours.fields}
    for key in sorted(gold_fields.keys() - ours_fields.keys()):
        diffs.append(SemanticDiff(DiffSeverity.ERROR, "field-missing", f"field {key[0]}:{key[1]} missing in ours"))
    for key in sorted(ours_fields.keys() - gold_fields.keys()):
        diffs.append(SemanticDiff(DiffSeverity.ERROR, "field-extra", f"field {key[0]}:{key[1]} extra in ours"))
    for key in sorted(gold_fields.keys() & ours_fields.keys()):
        gf = gold_fields[key]
        of = ours_fields[key]
        if gf.flags != of.flags:
            diffs.append(
                SemanticDiff(
                    DiffSeverity.ERROR,
                    "field-flags",
                    f"field {key[0]}:{key[1]} flags '{gf.flags}' vs '{of.flags}'",
                )
            )

    # --- methods ---
    gold_methods: dict[tuple[str, str], JavapMethod] = {(m.name, m.descriptor): m for m in gold.methods}
    ours_methods: dict[tuple[str, str], JavapMethod] = {(m.name, m.descriptor): m for m in ours.methods}
    gold_cp = _cp_comment_map(gold.constant_pool)
    ours_cp = _cp_comment_map(ours.constant_pool)

    for key in sorted(gold_methods.keys() - ours_methods.keys()):
        diffs.append(
            SemanticDiff(
                DiffSeverity.ERROR,
                "method-missing",
                f"method {key[0]}:{key[1]} missing in ours",
            )
        )
    for key in sorted(ours_methods.keys() - gold_methods.keys()):
        diffs.append(
            SemanticDiff(
                DiffSeverity.ERROR,
                "method-extra",
                f"method {key[0]}:{key[1]} extra in ours",
            )
        )
    for key in sorted(gold_methods.keys() & ours_methods.keys()):
        gm = gold_methods[key]
        om = ours_methods[key]
        label = f"{key[0]}:{key[1]}"
        if gm.flags != om.flags:
            diffs.append(
                SemanticDiff(
                    DiffSeverity.ERROR,
                    "method-flags",
                    f"method {label} flags '{gm.flags}' vs '{om.flags}'",
                )
            )
        if gm.code is not None and om.code is not None:
            _diff_instructions(gm.code, om.code, gold_cp, ours_cp, label, diffs)
        elif (gm.code is None) != (om.code is None):
            who = "gold" if gm.code is not None else "ours"
            diffs.append(
                SemanticDiff(
                    DiffSeverity.ERROR,
                    "method-code",
                    f"method {label}: only {who} has Code",
                )
            )

    return diffs


# ---------------------------------------------------------------------------
# Inline test
# ---------------------------------------------------------------------------

if __name__ == "__main__":
    _SAMPLE = (
        "Classfile /C:/Users/smith/AppData/Local/Temp/javap_test/HelloWorld.class\n"
        "  Last modified Mar 29, 2026; size 603 bytes\n"
        "  SHA-256 checksum 04bd23f88eb390fdf45051e294a07b80\n"
        '  Compiled from "HelloWorld.java"\n'
        "public class HelloWorld\n"
        "  minor version: 0\n"
        "  major version: 52\n"
        "  flags: (0x0021) ACC_PUBLIC, ACC_SUPER\n"
        "  this_class: #23                         // HelloWorld\n"
        "  super_class: #2                         // java/lang/Object\n"
        "  interfaces: 0, fields: 0, methods: 3, attributes: 1\n"
        "Constant pool:\n"
        '   #1 = Methodref          #2.#3          // java/lang/Object."<init>":()V\n'
        "   #2 = Class              #4             // java/lang/Object\n"
        '   #3 = NameAndType        #5:#6          // "<init>":()V\n'
        "   #4 = Utf8               java/lang/Object\n"
        "   #5 = Utf8               <init>\n"
        "   #6 = Utf8               ()V\n"
        "   #7 = Fieldref           #8.#9          // java/lang/System.out:Ljava/io/PrintStream;\n"
        "   #8 = Class              #10            // java/lang/System\n"
        "   #9 = NameAndType        #11:#12        // out:Ljava/io/PrintStream;\n"
        "  #10 = Utf8               java/lang/System\n"
        "  #11 = Utf8               out\n"
        "  #12 = Utf8               Ljava/io/PrintStream;\n"
        "  #13 = String             #14            // Hello from fixture\n"
        "  #14 = Utf8               Hello from fixture\n"
        "  #15 = Methodref          #16.#17        // java/io/PrintStream.println:(Ljava/lang/String;)V\n"
        "  #16 = Class              #18            // java/io/PrintStream\n"
        "  #17 = NameAndType        #19:#20        // println:(Ljava/lang/String;)V\n"
        "  #18 = Utf8               java/io/PrintStream\n"
        "  #19 = Utf8               println\n"
        "  #20 = Utf8               (Ljava/lang/String;)V\n"
        "  #21 = String             #22            // gift\n"
        "  #22 = Utf8               gift\n"
        "  #23 = Class              #24            // HelloWorld\n"
        "  #24 = Utf8               HelloWorld\n"
        "  #25 = Utf8               Code\n"
        "  #26 = Utf8               LineNumberTable\n"
        "  #27 = Utf8               main\n"
        "  #28 = Utf8               ([Ljava/lang/String;)V\n"
        "  #29 = Utf8               giveItToMe\n"
        "  #30 = Utf8               ()Ljava/lang/String;\n"
        "  #31 = Utf8               Deprecated\n"
        "  #32 = Utf8               RuntimeVisibleAnnotations\n"
        "  #33 = Utf8               Ljava/lang/Deprecated;\n"
        "  #34 = Utf8               SourceFile\n"
        "  #35 = Utf8               HelloWorld.java\n"
        "{\n"
        "  public HelloWorld();\n"
        "    descriptor: ()V\n"
        "    flags: (0x0001) ACC_PUBLIC\n"
        "    Code:\n"
        "      stack=1, locals=1, args_size=1\n"
        "         0: aload_0\n"
        '         1: invokespecial #1                  // Method java/lang/Object."<init>":()V\n'
        "         4: return\n"
        "      LineNumberTable:\n"
        "        line 1: 0\n"
        "\n"
        "  public static void main(java.lang.String[]);\n"
        "    descriptor: ([Ljava/lang/String;)V\n"
        "    flags: (0x0009) ACC_PUBLIC, ACC_STATIC\n"
        "    Code:\n"
        "      stack=2, locals=1, args_size=1\n"
        "         0: getstatic     #7                  // Field java/lang/System.out:Ljava/io/PrintStream;\n"
        "         3: ldc           #13                 // String Hello from fixture\n"
        "         5: invokevirtual #15                 // Method java/io/PrintStream.println:(Ljava/lang/String;)V\n"
        "         8: return\n"
        "      LineNumberTable:\n"
        "        line 3: 0\n"
        "        line 4: 8\n"
        "\n"
        "  public java.lang.String giveItToMe();\n"
        "    descriptor: ()Ljava/lang/String;\n"
        "    flags: (0x0001) ACC_PUBLIC\n"
        "    Code:\n"
        "      stack=1, locals=1, args_size=1\n"
        "         0: ldc           #21                 // String gift\n"
        "         2: areturn\n"
        "      LineNumberTable:\n"
        "        line 8: 0\n"
        "    Deprecated: true\n"
        "    RuntimeVisibleAnnotations:\n"
        "      0: #33()\n"
        "        java.lang.Deprecated\n"
        "}\n"
        'SourceFile: "HelloWorld.java"\n'
    )

    import copy

    cls = parse_javap(_SAMPLE)

    # --- structural assertions ---
    assert cls.major_version == 52, f"major_version: {cls.major_version}"
    assert cls.minor_version == 0, f"minor_version: {cls.minor_version}"
    assert cls.this_class == "HelloWorld", f"this_class: {cls.this_class}"
    assert cls.super_class == "java/lang/Object", f"super_class: {cls.super_class}"
    assert cls.flags == "(0x0021) ACC_PUBLIC, ACC_SUPER", f"flags: {cls.flags}"
    assert cls.interfaces == [], f"interfaces: {cls.interfaces}"

    # constant pool
    assert len(cls.constant_pool) == 35, f"cp size: {len(cls.constant_pool)}"
    assert cls.constant_pool[0].index == 1
    assert cls.constant_pool[0].tag == "Methodref"
    assert cls.constant_pool[0].comment == 'java/lang/Object."<init>":()V'

    # methods
    assert len(cls.methods) == 3, f"methods: {len(cls.methods)}"
    assert cls.methods[0].name == "HelloWorld", f"m0 name: {cls.methods[0].name}"
    assert cls.methods[0].descriptor == "()V"
    assert cls.methods[1].name == "main"
    assert cls.methods[1].descriptor == "([Ljava/lang/String;)V"
    assert cls.methods[2].name == "giveItToMe"
    assert cls.methods[2].descriptor == "()Ljava/lang/String;"

    # code block
    init_code = cls.methods[0].code
    assert init_code is not None
    assert init_code.max_stack == 1
    assert init_code.max_locals == 1
    assert len(init_code.instructions) == 3
    assert init_code.instructions[0].mnemonic == "aload_0"
    assert init_code.instructions[1].mnemonic == "invokespecial"
    assert init_code.instructions[1].operands == "#1"
    assert init_code.instructions[1].comment == 'Method java/lang/Object."<init>":()V'
    assert init_code.instructions[2].mnemonic == "return"

    main_code = cls.methods[1].code
    assert main_code is not None
    assert len(main_code.instructions) == 4
    assert main_code.instructions[0].mnemonic == "getstatic"
    assert main_code.instructions[2].mnemonic == "invokevirtual"

    # fields (none in HelloWorld)
    assert len(cls.fields) == 0

    # --- self-diff should be empty ---
    diffs = semantic_diff(cls, cls)
    assert len(diffs) == 0, f"self-diff produced {len(diffs)} diffs: {diffs}"

    # --- diff with a modified copy ---
    modified = copy.deepcopy(cls)
    modified.major_version = 65
    diffs = semantic_diff(cls, modified)
    assert any(d.severity == DiffSeverity.ERROR and d.category == "class-version" for d in diffs)

    # Remove a method to test method-missing detection
    modified2 = copy.deepcopy(cls)
    modified2.methods = modified2.methods[:2]
    diffs2 = semantic_diff(cls, modified2)
    assert any(d.category == "method-missing" for d in diffs2)

    # CP-index-only difference (same symbol, different index)
    modified3 = copy.deepcopy(cls)
    assert modified3.methods[0].code is not None
    modified3.methods[0].code.instructions[1].operands = "#99"
    modified3.constant_pool.append(JavapCPEntry(99, "Methodref", "#X.#Y", 'java/lang/Object."<init>":()V'))
    diffs3 = semantic_diff(cls, modified3)
    assert any(d.severity == DiffSeverity.INFO and d.category == "cp-ordering" for d in diffs3), f"diffs3: {diffs3}"

    print("All inline tests passed.")
    sys.exit(0)
