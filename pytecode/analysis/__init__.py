"""Control-flow graph construction and stack/local simulation.

This module re-exports from either the Cython-accelerated implementation
or the pure-Python fallback depending on availability and the
``PYTECODE_BLOCK_CYTHON`` environment variable.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pytecode.analysis._analysis_py import (
        OPCODE_EFFECTS as OPCODE_EFFECTS,
    )
    from pytecode.analysis._analysis_py import (
        AnalysisError as AnalysisError,
    )
    from pytecode.analysis._analysis_py import (
        BasicBlock as BasicBlock,
    )
    from pytecode.analysis._analysis_py import (
        ControlFlowGraph as ControlFlowGraph,
    )
    from pytecode.analysis._analysis_py import (
        ExceptionEdge as ExceptionEdge,
    )
    from pytecode.analysis._analysis_py import (
        FrameComputationResult as FrameComputationResult,
    )
    from pytecode.analysis._analysis_py import (
        FrameState as FrameState,
    )
    from pytecode.analysis._analysis_py import (
        InvalidLocalError as InvalidLocalError,
    )
    from pytecode.analysis._analysis_py import (
        OpcodeEffect as OpcodeEffect,
    )
    from pytecode.analysis._analysis_py import (
        SimulationResult as SimulationResult,
    )
    from pytecode.analysis._analysis_py import (
        StackUnderflowError as StackUnderflowError,
    )
    from pytecode.analysis._analysis_py import (
        TypeMergeError as TypeMergeError,
    )
    from pytecode.analysis._analysis_py import (
        VDouble as VDouble,
    )
    from pytecode.analysis._analysis_py import (
        VFloat as VFloat,
    )
    from pytecode.analysis._analysis_py import (
        VInteger as VInteger,
    )
    from pytecode.analysis._analysis_py import (
        VLong as VLong,
    )
    from pytecode.analysis._analysis_py import (
        VNull as VNull,
    )
    from pytecode.analysis._analysis_py import (
        VObject as VObject,
    )
    from pytecode.analysis._analysis_py import (
        VTop as VTop,
    )
    from pytecode.analysis._analysis_py import (
        VType as VType,
    )
    from pytecode.analysis._analysis_py import (
        VUninitialized as VUninitialized,
    )
    from pytecode.analysis._analysis_py import (
        VUninitializedThis as VUninitializedThis,
    )
    from pytecode.analysis._analysis_py import (
        _select_frame as _select_frame,
    )
    from pytecode.analysis._analysis_py import (
        _vtype_to_vti as _vtype_to_vti,
    )
    from pytecode.analysis._analysis_py import (
        build_cfg as build_cfg,
    )
    from pytecode.analysis._analysis_py import (
        compute_frames as compute_frames,
    )
    from pytecode.analysis._analysis_py import (
        compute_maxs as compute_maxs,
    )
    from pytecode.analysis._analysis_py import (
        initial_frame as initial_frame,
    )
    from pytecode.analysis._analysis_py import (
        is_category2 as is_category2,
    )
    from pytecode.analysis._analysis_py import (
        is_reference as is_reference,
    )
    from pytecode.analysis._analysis_py import (
        merge_vtypes as merge_vtypes,
    )
    from pytecode.analysis._analysis_py import (
        simulate as simulate,
    )
    from pytecode.analysis._analysis_py import (
        vtype_from_descriptor as vtype_from_descriptor,
    )
    from pytecode.analysis._analysis_py import (
        vtype_from_field_descriptor_str as vtype_from_field_descriptor_str,
    )
else:
    from pytecode._internal.cython_import import import_cython_module

    _impl = import_cython_module(
        "pytecode.analysis._analysis_cy",
        "pytecode.analysis._analysis_py",
    )

    AnalysisError = _impl.AnalysisError
    BasicBlock = _impl.BasicBlock
    ControlFlowGraph = _impl.ControlFlowGraph
    ExceptionEdge = _impl.ExceptionEdge
    FrameComputationResult = _impl.FrameComputationResult
    FrameState = _impl.FrameState
    InvalidLocalError = _impl.InvalidLocalError
    OPCODE_EFFECTS = _impl.OPCODE_EFFECTS
    OpcodeEffect = _impl.OpcodeEffect
    SimulationResult = _impl.SimulationResult
    StackUnderflowError = _impl.StackUnderflowError
    TypeMergeError = _impl.TypeMergeError
    VDouble = _impl.VDouble
    VFloat = _impl.VFloat
    VInteger = _impl.VInteger
    VLong = _impl.VLong
    VNull = _impl.VNull
    VObject = _impl.VObject
    VTop = _impl.VTop
    VUninitialized = _impl.VUninitialized
    VUninitializedThis = _impl.VUninitializedThis
    _select_frame = _impl._select_frame
    _vtype_to_vti = _impl._vtype_to_vti
    build_cfg = _impl.build_cfg
    compute_frames = _impl.compute_frames
    compute_maxs = _impl.compute_maxs
    initial_frame = _impl.initial_frame
    is_category2 = _impl.is_category2
    is_reference = _impl.is_reference
    merge_vtypes = _impl.merge_vtypes
    simulate = _impl.simulate
    vtype_from_descriptor = _impl.vtype_from_descriptor
    vtype_from_field_descriptor_str = _impl.vtype_from_field_descriptor_str

    # VType is a PEP 695 type alias — not available from Cython build.
    from pytecode.analysis._analysis_py import VType as VType  # noqa: E402

__all__ = [
    # Errors
    "AnalysisError",
    "InvalidLocalError",
    "StackUnderflowError",
    "TypeMergeError",
    # Verification types
    "VDouble",
    "VFloat",
    "VInteger",
    "VLong",
    "VNull",
    "VObject",
    "VTop",
    "VType",
    "VUninitialized",
    "VUninitializedThis",
    # VType helpers
    "is_category2",
    "is_reference",
    "merge_vtypes",
    "vtype_from_descriptor",
    "vtype_from_field_descriptor_str",
    # Frame state
    "FrameState",
    "initial_frame",
    # Opcode metadata
    "OPCODE_EFFECTS",
    "OpcodeEffect",
    # CFG
    "BasicBlock",
    "ControlFlowGraph",
    "ExceptionEdge",
    "build_cfg",
    # Simulation
    "SimulationResult",
    "simulate",
    # Frame computation
    "FrameComputationResult",
    "compute_frames",
    "compute_maxs",
]
