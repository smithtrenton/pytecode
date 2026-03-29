from __future__ import annotations

import shutil
from pathlib import Path

import pytest

from pytecode import ClassModel
from pytecode.analysis import build_cfg
from pytecode.instructions import InsnInfoType
from tests.cfg_oracle import (
    NormalizedBlock,
    NormalizedCfg,
    OracleInsn,
    OracleMethodCfg,
    compare_cfgs,
    normalize_pytecode_cfg,
    normalize_to_blocks,
    parse_oracle_output,
)
from tests.helpers import compile_java_resource, run_oracle

_CFG_FIXTURE_METHODS = (
    "straightLine",
    "emptyMethod",
    "ifElse",
    "ifNoElse",
    "forLoop",
    "whileLoop",
    "nestedLoops",
    "denseSwitch",
    "sparseSwitch",
    "tryCatchSingle",
    "tryCatchMultiple",
    "tryCatchFinally",
    "nestedTryCatch",
    "longArithmetic",
    "doubleArithmetic",
    "mixedConversions",
    "createObject",
    "createString",
    "createIntArray",
    "createStringArray",
    "arrayAccess",
    "arrayStore",
    "isString",
    "castToString",
    "multipleReturns",
    "methodCalls",
    "synchronizedMethod",
    "nullCheck",
    "complexCondition",
    "create2DArray",
    "voidMethod",
    "compareLongs",
    "compareDoubles",
    "throwException",
)

_EDGE_CASE_METHODS = (
    "overlappingHandlers",
    "catchAllHandler",
    "multiCatch",
    "synchronizedBlock",
    "unreachableAfterGoto",
    "unreachableAfterReturn",
    "constructorInBranch",
    "largeTableSwitch",
    "largeLookupSwitch",
    "handlerThatThrows",
    "tryWithResources",
    "deeplyNestedTryCatch",
    "loopWithHandler",
    "switchFallThrough",
)


def _oracle_method_map(class_file: Path) -> dict[str, OracleMethodCfg]:
    if shutil.which("java") is None or shutil.which("javac") is None:
        pytest.skip("Oracle CFG tests require java and javac")
    try:
        methods = parse_oracle_output(run_oracle(class_file))
    except AssertionError as exc:
        pytest.skip(f"ASM oracle unavailable: {exc}")
    return {method.method_name: method for method in methods}


def _find_method(model: ClassModel, name: str):
    return next(method for method in model.methods if method.name == name)


def test_normalize_to_blocks_handles_conditional_branch_shape() -> None:
    oracle = OracleMethodCfg(
        class_name="Test",
        method_name="ifElse",
        method_descriptor="()I",
        instructions=[
            OracleInsn(0, InsnInfoType.ICONST_0, "ICONST_0"),
            OracleInsn(1, InsnInfoType.IFEQ, "IFEQ"),
            OracleInsn(2, InsnInfoType.ICONST_1, "ICONST_1"),
            OracleInsn(3, InsnInfoType.IRETURN, "IRETURN"),
            OracleInsn(4, InsnInfoType.ICONST_2, "ICONST_2"),
            OracleInsn(5, InsnInfoType.IRETURN, "IRETURN"),
        ],
        normal_edges=[(0, 1), (1, 2), (1, 4), (2, 3), (4, 5)],
        exception_edges=[],
        try_catch_blocks=[],
    )

    normalized = normalize_to_blocks(oracle)

    assert normalized == NormalizedCfg(
        entry_first_insn=0,
        blocks=(
            NormalizedBlock(0, 1, 2, frozenset({2, 4}), frozenset()),
            NormalizedBlock(2, 3, 2, frozenset(), frozenset()),
            NormalizedBlock(4, 5, 2, frozenset(), frozenset()),
        ),
    )


def test_normalize_to_blocks_splits_on_try_catch_boundaries() -> None:
    oracle = OracleMethodCfg(
        class_name="Test",
        method_name="tryCatch",
        method_descriptor="()I",
        instructions=[
            OracleInsn(0, InsnInfoType.ICONST_1, "ICONST_1"),
            OracleInsn(1, InsnInfoType.IDIV, "IDIV"),
            OracleInsn(2, InsnInfoType.IRETURN, "IRETURN"),
            OracleInsn(3, InsnInfoType.ICONST_M1, "ICONST_M1"),
            OracleInsn(4, InsnInfoType.IRETURN, "IRETURN"),
        ],
        normal_edges=[(0, 1), (1, 2), (3, 4)],
        exception_edges=[(1, 3, "java/lang/ArithmeticException")],
        try_catch_blocks=[(1, 2, 3, "java/lang/ArithmeticException")],
    )

    normalized = normalize_to_blocks(oracle)

    assert normalized == NormalizedCfg(
        entry_first_insn=0,
        blocks=(
            NormalizedBlock(0, 0, 1, frozenset({1}), frozenset()),
            NormalizedBlock(
                1,
                1,
                1,
                frozenset({2}),
                frozenset({(3, "java/lang/ArithmeticException")}),
            ),
            NormalizedBlock(2, 2, 1, frozenset(), frozenset()),
            NormalizedBlock(3, 4, 2, frozenset(), frozenset()),
        ),
    )


def test_compare_cfgs_reports_mismatches() -> None:
    pytecode_cfg = NormalizedCfg(
        entry_first_insn=0,
        blocks=(
            NormalizedBlock(0, 1, 2, frozenset({2}), frozenset()),
            NormalizedBlock(2, 2, 1, frozenset(), frozenset()),
        ),
    )
    oracle_cfg = NormalizedCfg(
        entry_first_insn=0,
        blocks=(
            NormalizedBlock(0, 1, 2, frozenset({3}), frozenset()),
            NormalizedBlock(3, 3, 1, frozenset(), frozenset()),
        ),
    )

    differences = compare_cfgs(pytecode_cfg, oracle_cfg)

    assert differences == [
        "missing pytecode block starting at instruction 3",
        "unexpected pytecode block starting at instruction 2",
        "block 0 normal successors mismatch: pytecode=[2], oracle=[3]",
    ]


@pytest.mark.oracle
class TestCfgOracleIntegration:
    """Differential CFG tests: pytecode vs ASM oracle."""

    @pytest.fixture(scope="class")
    def cfg_fixture_class(self, tmp_path_factory: pytest.TempPathFactory) -> Path:
        tmp_path = tmp_path_factory.mktemp("cfg-fixture-oracle")
        return compile_java_resource(tmp_path, "CfgFixture.java")

    @pytest.fixture(scope="class")
    def cfg_fixture_model(self, cfg_fixture_class: Path) -> ClassModel:
        return ClassModel.from_bytes(cfg_fixture_class.read_bytes())

    @pytest.fixture(scope="class")
    def cfg_fixture_oracle(self, cfg_fixture_class: Path) -> dict[str, OracleMethodCfg]:
        return _oracle_method_map(cfg_fixture_class)

    @pytest.fixture(scope="class")
    def edge_case_class(self, tmp_path_factory: pytest.TempPathFactory) -> Path:
        tmp_path = tmp_path_factory.mktemp("cfg-edge-oracle")
        return compile_java_resource(tmp_path, "CfgEdgeCaseFixture.java")

    @pytest.fixture(scope="class")
    def edge_case_model(self, edge_case_class: Path) -> ClassModel:
        return ClassModel.from_bytes(edge_case_class.read_bytes())

    @pytest.fixture(scope="class")
    def edge_case_oracle(self, edge_case_class: Path) -> dict[str, OracleMethodCfg]:
        return _oracle_method_map(edge_case_class)

    @pytest.mark.parametrize("method_name", _CFG_FIXTURE_METHODS)
    def test_cfg_fixture_matches_oracle(
        self,
        cfg_fixture_model: ClassModel,
        cfg_fixture_oracle: dict[str, OracleMethodCfg],
        method_name: str,
    ) -> None:
        method = _find_method(cfg_fixture_model, method_name)
        assert method.code is not None

        differences = compare_cfgs(
            normalize_pytecode_cfg(build_cfg(method.code)),
            normalize_to_blocks(cfg_fixture_oracle[method_name]),
        )

        assert differences == []

    @pytest.mark.parametrize("method_name", _EDGE_CASE_METHODS)
    def test_edge_case_fixture_matches_oracle(
        self,
        edge_case_model: ClassModel,
        edge_case_oracle: dict[str, OracleMethodCfg],
        method_name: str,
    ) -> None:
        method = _find_method(edge_case_model, method_name)
        assert method.code is not None

        differences = compare_cfgs(
            normalize_pytecode_cfg(build_cfg(method.code)),
            normalize_to_blocks(edge_case_oracle[method_name]),
        )

        assert differences == []
