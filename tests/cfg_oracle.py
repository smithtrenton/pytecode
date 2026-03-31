from __future__ import annotations

import json
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from typing import cast

from pytecode.analysis import ControlFlowGraph
from pytecode.instructions import Branch, BranchW, InsnInfoType, LookupSwitch, TableSwitch

type OracleNormalEdge = tuple[int, int]
type OracleExceptionEdge = tuple[int, int, str | None]
type OracleTryCatchBlock = tuple[int, int, int, str | None]
type NormalizedExceptionHandler = tuple[int, str | None]

_TERMINAL_OPCODES = frozenset(
    {
        InsnInfoType.RET,
        InsnInfoType.ATHROW,
        InsnInfoType.IRETURN,
        InsnInfoType.LRETURN,
        InsnInfoType.FRETURN,
        InsnInfoType.DRETURN,
        InsnInfoType.ARETURN,
        InsnInfoType.RETURN,
    }
)


@dataclass(frozen=True, slots=True)
class OracleInsn:
    index: int
    opcode: int
    mnemonic: str


@dataclass(frozen=True, slots=True)
class OracleMethodCfg:
    """Parsed oracle output for one method."""

    class_name: str
    method_name: str
    method_descriptor: str
    instructions: list[OracleInsn]
    normal_edges: list[OracleNormalEdge]
    exception_edges: list[OracleExceptionEdge]
    try_catch_blocks: list[OracleTryCatchBlock]


@dataclass(frozen=True, slots=True)
class NormalizedBlock:
    """Oracle-derived or pytecode-derived basic block."""

    first_insn_index: int
    last_insn_index: int
    insn_count: int
    normal_successor_first_insns: frozenset[int]
    exception_handlers: frozenset[NormalizedExceptionHandler]


@dataclass(frozen=True, slots=True)
class NormalizedCfg:
    """Control-flow graph normalized to instruction-index-addressed blocks."""

    blocks: tuple[NormalizedBlock, ...]
    entry_first_insn: int


def parse_oracle_output(json_text: str | Mapping[str, object]) -> list[OracleMethodCfg]:
    """Parse JSON output from ``RecordingAnalyzer``."""

    if isinstance(json_text, str):
        raw_document = json.loads(json_text)
    else:
        raw_document = dict(json_text)
    document = _expect_mapping(raw_document, context="oracle output")

    default_class_name = _expect_str(document.get("className"), context="oracle output.className")
    raw_methods = _expect_sequence(document.get("methods"), context="oracle output.methods")

    methods: list[OracleMethodCfg] = []
    for method_index, raw_method in enumerate(raw_methods):
        method = _expect_mapping(raw_method, context=f"oracle output.methods[{method_index}]")
        class_name = default_class_name
        if "className" in method:
            class_name = _expect_str(method["className"], context=f"oracle output.methods[{method_index}].className")

        instructions = _parse_instructions(
            _expect_sequence(method.get("instructions"), context=f"oracle output.methods[{method_index}].instructions"),
            context=f"oracle output.methods[{method_index}].instructions",
        )
        methods.append(
            OracleMethodCfg(
                class_name=class_name,
                method_name=_expect_str(
                    method.get("methodName"),
                    context=f"oracle output.methods[{method_index}].methodName",
                ),
                method_descriptor=_expect_str(
                    method.get("methodDescriptor"),
                    context=f"oracle output.methods[{method_index}].methodDescriptor",
                ),
                instructions=instructions,
                normal_edges=_parse_normal_edges(
                    _expect_sequence(
                        method.get("normalEdges"),
                        context=f"oracle output.methods[{method_index}].normalEdges",
                    ),
                    context=f"oracle output.methods[{method_index}].normalEdges",
                ),
                exception_edges=_parse_exception_edges(
                    _expect_sequence(
                        method.get("exceptionEdges"),
                        context=f"oracle output.methods[{method_index}].exceptionEdges",
                    ),
                    context=f"oracle output.methods[{method_index}].exceptionEdges",
                ),
                try_catch_blocks=_parse_try_catch_blocks(
                    _expect_sequence(
                        method.get("tryCatchBlocks"),
                        context=f"oracle output.methods[{method_index}].tryCatchBlocks",
                    ),
                    context=f"oracle output.methods[{method_index}].tryCatchBlocks",
                ),
            )
        )
    return methods


def normalize_to_blocks(oracle: OracleMethodCfg) -> NormalizedCfg:
    """Convert instruction-level oracle edges into block-level expectations."""

    instruction_count = len(oracle.instructions)
    if instruction_count == 0:
        return NormalizedCfg(blocks=tuple(), entry_first_insn=0)

    actual_indices = [instruction.index for instruction in oracle.instructions]
    expected_indices = list(range(instruction_count))
    if actual_indices != expected_indices:
        raise ValueError(
            f"Oracle instruction indices for {oracle.method_name}{oracle.method_descriptor} "
            f"must be dense 0..{instruction_count - 1}, got {actual_indices!r}"
        )

    leaders: set[int] = {0}
    for from_insn, to_insn in oracle.normal_edges:
        if to_insn != from_insn + 1:
            leaders.add(to_insn)
    for _, handler_insn, _ in oracle.exception_edges:
        leaders.add(handler_insn)
    for start_index, end_index, handler_index, _ in oracle.try_catch_blocks:
        leaders.add(start_index)
        leaders.add(handler_index)
        if end_index < instruction_count:
            leaders.add(end_index)
    for instruction in oracle.instructions:
        if _ends_basic_block(instruction.opcode) and instruction.index + 1 < instruction_count:
            leaders.add(instruction.index + 1)

    sorted_leaders = sorted(leaders)
    block_ranges: list[tuple[int, int]] = []
    for leader_index, start in enumerate(sorted_leaders):
        end = instruction_count - 1
        if leader_index + 1 < len(sorted_leaders):
            end = sorted_leaders[leader_index + 1] - 1
        block_ranges.append((start, end))

    block_start_by_insn: dict[int, int] = {}
    for start, end in block_ranges:
        for insn_index in range(start, end + 1):
            block_start_by_insn[insn_index] = start

    outgoing_normal_edges: dict[int, list[int]] = {}
    for from_insn, to_insn in oracle.normal_edges:
        outgoing_normal_edges.setdefault(from_insn, []).append(to_insn)

    exception_edges_by_source: dict[int, list[tuple[int, str | None]]] = {}
    for from_insn, handler_insn, catch_type in oracle.exception_edges:
        exception_edges_by_source.setdefault(from_insn, []).append((handler_insn, catch_type))

    normalized_blocks: list[NormalizedBlock] = []
    for start, end in block_ranges:
        normal_successors = frozenset(block_start_by_insn[to_insn] for to_insn in outgoing_normal_edges.get(end, []))
        exception_handlers = frozenset(
            (
                block_start_by_insn[handler_insn],
                catch_type,
            )
            for insn_index in range(start, end + 1)
            for handler_insn, catch_type in exception_edges_by_source.get(insn_index, [])
        )
        normalized_blocks.append(
            NormalizedBlock(
                first_insn_index=start,
                last_insn_index=end,
                insn_count=end - start + 1,
                normal_successor_first_insns=normal_successors,
                exception_handlers=exception_handlers,
            )
        )

    return NormalizedCfg(blocks=tuple(normalized_blocks), entry_first_insn=normalized_blocks[0].first_insn_index)


def normalize_pytecode_cfg(cfg: ControlFlowGraph) -> NormalizedCfg:
    """Convert ``pytecode`` CFG blocks into the normalized instruction-index form."""

    # build_cfg guarantees every block (including entry) has at least one
    # instruction, so empty-block filtering is purely defensive.
    non_empty_blocks = [block for block in cfg.blocks if block.instructions]
    if not non_empty_blocks:
        return NormalizedCfg(blocks=tuple(), entry_first_insn=0)

    instruction_index_by_identity = {
        id(instruction): index
        for index, instruction in enumerate(
            instruction for block in non_empty_blocks for instruction in block.instructions
        )
    }
    block_by_id = {block.id: block for block in non_empty_blocks}

    normalized_blocks: list[NormalizedBlock] = []
    for block in non_empty_blocks:
        first_insn_index = instruction_index_by_identity[id(block.instructions[0])]
        last_insn_index = instruction_index_by_identity[id(block.instructions[-1])]
        # Successors/handlers always point to non-empty blocks because
        # build_cfg only creates blocks that start at leader instructions.
        normal_successors = frozenset(
            instruction_index_by_identity[id(block_by_id[successor_id].instructions[0])]
            for successor_id in block.successor_ids
            if successor_id in block_by_id
        )
        exception_handlers = frozenset(
            (
                instruction_index_by_identity[id(block_by_id[handler_id].instructions[0])],
                catch_type,
            )
            for handler_id, catch_type in block.exception_handler_ids
            if handler_id in block_by_id
        )
        normalized_blocks.append(
            NormalizedBlock(
                first_insn_index=first_insn_index,
                last_insn_index=last_insn_index,
                insn_count=len(block.instructions),
                normal_successor_first_insns=normal_successors,
                exception_handlers=exception_handlers,
            )
        )

    entry_first_insn = instruction_index_by_identity[id(cfg.entry.instructions[0])]
    return NormalizedCfg(blocks=tuple(normalized_blocks), entry_first_insn=entry_first_insn)


def compare_cfgs(pytecode_cfg: NormalizedCfg, oracle_cfg: NormalizedCfg) -> list[str]:
    """Compare normalized CFGs, returning a list of mismatch descriptions."""

    differences: list[str] = []
    if pytecode_cfg.entry_first_insn != oracle_cfg.entry_first_insn:
        differences.append(
            "entry block mismatch: "
            f"pytecode starts at {pytecode_cfg.entry_first_insn}, "
            f"oracle starts at {oracle_cfg.entry_first_insn}"
        )

    if len(pytecode_cfg.blocks) != len(oracle_cfg.blocks):
        differences.append(
            f"block count mismatch: pytecode has {len(pytecode_cfg.blocks)}, oracle has {len(oracle_cfg.blocks)}"
        )

    pytecode_blocks = {block.first_insn_index: block for block in pytecode_cfg.blocks}
    oracle_blocks = {block.first_insn_index: block for block in oracle_cfg.blocks}

    missing_from_pytecode = sorted(oracle_blocks.keys() - pytecode_blocks.keys())
    for first_insn in missing_from_pytecode:
        differences.append(f"missing pytecode block starting at instruction {first_insn}")

    extra_in_pytecode = sorted(pytecode_blocks.keys() - oracle_blocks.keys())
    for first_insn in extra_in_pytecode:
        differences.append(f"unexpected pytecode block starting at instruction {first_insn}")

    for first_insn in sorted(pytecode_blocks.keys() & oracle_blocks.keys()):
        pytecode_block = pytecode_blocks[first_insn]
        oracle_block = oracle_blocks[first_insn]

        if (
            pytecode_block.last_insn_index != oracle_block.last_insn_index
            or pytecode_block.insn_count != oracle_block.insn_count
        ):
            differences.append(
                f"block {first_insn} span mismatch: "
                f"pytecode={_format_block_span(pytecode_block)}, "
                f"oracle={_format_block_span(oracle_block)}"
            )

        if pytecode_block.normal_successor_first_insns != oracle_block.normal_successor_first_insns:
            differences.append(
                f"block {first_insn} normal successors mismatch: "
                f"pytecode={sorted(pytecode_block.normal_successor_first_insns)}, "
                f"oracle={sorted(oracle_block.normal_successor_first_insns)}"
            )

        if pytecode_block.exception_handlers != oracle_block.exception_handlers:
            differences.append(
                f"block {first_insn} exception handlers mismatch: "
                f"pytecode={_format_exception_handlers(pytecode_block.exception_handlers)}, "
                f"oracle={_format_exception_handlers(oracle_block.exception_handlers)}"
            )

    return differences


def _parse_instructions(raw_instructions: Sequence[object], *, context: str) -> list[OracleInsn]:
    instructions: list[OracleInsn] = []
    for instruction_index, raw_instruction in enumerate(raw_instructions):
        instruction = _expect_mapping(raw_instruction, context=f"{context}[{instruction_index}]")
        instructions.append(
            OracleInsn(
                index=_expect_int(instruction.get("index"), context=f"{context}[{instruction_index}].index"),
                opcode=_expect_int(instruction.get("opcode"), context=f"{context}[{instruction_index}].opcode"),
                mnemonic=_expect_str(
                    instruction.get("mnemonic"),
                    context=f"{context}[{instruction_index}].mnemonic",
                ),
            )
        )
    return instructions


def _parse_normal_edges(raw_edges: Sequence[object], *, context: str) -> list[OracleNormalEdge]:
    edges: list[OracleNormalEdge] = []
    for edge_index, raw_edge in enumerate(raw_edges):
        edge = _expect_mapping(raw_edge, context=f"{context}[{edge_index}]")
        edges.append(
            (
                _expect_int(edge.get("from"), context=f"{context}[{edge_index}].from"),
                _expect_int(edge.get("to"), context=f"{context}[{edge_index}].to"),
            )
        )
    return edges


def _parse_exception_edges(raw_edges: Sequence[object], *, context: str) -> list[OracleExceptionEdge]:
    edges: list[OracleExceptionEdge] = []
    for edge_index, raw_edge in enumerate(raw_edges):
        edge = _expect_mapping(raw_edge, context=f"{context}[{edge_index}]")
        edges.append(
            (
                _expect_int(edge.get("from"), context=f"{context}[{edge_index}].from"),
                _expect_int(edge.get("handler"), context=f"{context}[{edge_index}].handler"),
                _expect_nullable_str(
                    edge.get("catchType"),
                    context=f"{context}[{edge_index}].catchType",
                ),
            )
        )
    return edges


def _parse_try_catch_blocks(raw_blocks: Sequence[object], *, context: str) -> list[OracleTryCatchBlock]:
    blocks: list[OracleTryCatchBlock] = []
    for block_index, raw_block in enumerate(raw_blocks):
        block = _expect_mapping(raw_block, context=f"{context}[{block_index}]")
        blocks.append(
            (
                _expect_int(block.get("startIndex"), context=f"{context}[{block_index}].startIndex"),
                _expect_int(block.get("endIndex"), context=f"{context}[{block_index}].endIndex"),
                _expect_int(block.get("handlerIndex"), context=f"{context}[{block_index}].handlerIndex"),
                _expect_nullable_str(
                    block.get("catchType"),
                    context=f"{context}[{block_index}].catchType",
                ),
            )
        )
    return blocks


def _expect_mapping(value: object, *, context: str) -> Mapping[str, object]:
    if not isinstance(value, Mapping):
        raise ValueError(f"{context} must be a JSON object")
    return cast(Mapping[str, object], value)


def _expect_sequence(value: object, *, context: str) -> Sequence[object]:
    if not isinstance(value, Sequence) or isinstance(value, (str, bytes, bytearray)):
        raise ValueError(f"{context} must be a JSON array")
    return cast(Sequence[object], value)


def _expect_str(value: object, *, context: str) -> str:
    if not isinstance(value, str):
        raise ValueError(f"{context} must be a string")
    return value


def _expect_int(value: object, *, context: str) -> int:
    if not isinstance(value, int) or isinstance(value, bool):
        raise ValueError(f"{context} must be an integer")
    return value


def _expect_nullable_str(value: object, *, context: str) -> str | None:
    if value is None:
        return None
    return _expect_str(value, context=context)


def _ends_basic_block(opcode: int) -> bool:
    insn_type = InsnInfoType(opcode)
    if insn_type in _TERMINAL_OPCODES:
        return True
    return insn_type.instinfo in {Branch, BranchW, LookupSwitch, TableSwitch}


def _format_block_span(block: NormalizedBlock) -> str:
    return f"{block.first_insn_index}..{block.last_insn_index} ({block.insn_count} insns)"


def _format_exception_handlers(handlers: frozenset[NormalizedExceptionHandler]) -> list[NormalizedExceptionHandler]:
    return sorted(handlers, key=lambda item: (item[0], "" if item[1] is None else item[1]))
