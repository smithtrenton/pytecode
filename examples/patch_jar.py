from __future__ import annotations

import argparse
import json
import re
from collections.abc import Sequence
from dataclasses import dataclass
from enum import IntFlag
from pathlib import Path
from re import Pattern

from pytecode import JarFile
from pytecode.archive import (
    DebugInfoPolicy,
    FrameComputationMode,
    normalize_debug_info_policy,
)
from pytecode.classfile.constants import ClassAccessFlag, FieldAccessFlag, MethodAccessFlag
from pytecode.model import (
    BranchInsn,
    ClassModel,
    FieldInsn,
    IIncInsn,
    Label,
    LdcInsn,
    LookupSwitchInsn,
    MethodInsn,
    RawInsn,
    TableSwitchInsn,
    TypeInsn,
    VarInsn,
)
from pytecode.transforms import CodeTransform, InsnMatcher, MethodMatcher, PipelineBuilder, class_named

CLASS_VISIBILITY_FLAGS = int(ClassAccessFlag.PUBLIC)
MEMBER_VISIBILITY_FLAGS = int(FieldAccessFlag.PUBLIC | FieldAccessFlag.PRIVATE | FieldAccessFlag.PROTECTED)


@dataclass(slots=True)
class PatchOptions:
    debug_info: DebugInfoPolicy = DebugInfoPolicy.PRESERVE
    frame_mode: FrameComputationMode = FrameComputationMode.PRESERVE


@dataclass(slots=True)
class CompiledCodeAction:
    transform: CodeTransform
    matcher: InsnMatcher | None = None
    pattern: list[InsnMatcher] | None = None


@dataclass(slots=True)
class CompiledRule:
    name: str
    kind: str
    owner: dict[str, object]
    matcher: dict[str, object]
    action: dict[str, object] | None
    code_actions: list[CompiledCodeAction]


def patch_jar(jar_path: str | Path, output_path: str | Path, rules_path: str | Path) -> dict[str, object]:
    plan = _load_patch_plan(rules_path)
    jar = JarFile(jar_path)
    class_entries = sum(1 for jar_info in jar.files.values() if jar_info.filename.endswith(".class"))
    resource_entries = sum(1 for jar_info in jar.files.values() if not jar_info.filename.endswith(".class"))

    compiled_rules = [_compile_rule(index, rule) for index, rule in enumerate(plan["rules"])]
    rule_reports = [
        {
            "name": rule.name,
            "kind": rule.kind,
            "matched_classes": 0,
            "changed_classes": 0,
            "matched_targets": 0,
            "changed_targets": 0,
        }
        for rule in compiled_rules
    ]

    frame_mode = plan["options"].frame_mode
    debug_info = plan["options"].debug_info

    for jar_info in list(jar.files.values()):
        if not jar_info.filename.endswith(".class"):
            continue

        model = ClassModel.from_bytes(jar_info.bytes)
        class_changed = False
        for index, rule in enumerate(compiled_rules):
            matched_targets, changed_targets = _apply_rule(model, rule)
            if matched_targets == 0:
                continue
            report = rule_reports[index]
            report["matched_classes"] += 1
            report["matched_targets"] += matched_targets
            if changed_targets > 0:
                report["changed_classes"] += 1
                report["changed_targets"] += changed_targets
                class_changed = True

        if not class_changed:
            continue

        updated_bytes = model.to_bytes_with_options(
            frame_mode=frame_mode,
            debug_info=debug_info.value,
        )
        if updated_bytes != jar_info.bytes:
            jar.add_file(jar_info.filename, updated_bytes, zipinfo=jar_info.zipinfo)

    jar.rewrite(output_path)
    return {
        "input_jar": str(Path(jar_path)),
        "output_jar": str(Path(output_path)),
        "rules_path": str(Path(rules_path)),
        "class_entries": class_entries,
        "resource_entries": resource_entries,
        "debug_info": debug_info.value,
        "frame_mode": plan["options"].frame_mode.value,
        "rules": rule_reports,
    }


def _load_patch_plan(rules_path: str | Path) -> dict[str, object]:
    plan = json.loads(Path(rules_path).read_text(encoding="utf-8"))
    if not isinstance(plan, dict):
        raise ValueError("patch plan must be a JSON object")

    options_raw = plan.get("options", {})
    if not isinstance(options_raw, dict):
        raise ValueError("patch plan options must be an object")
    debug_info = normalize_debug_info_policy(options_raw.get("debug_info", DebugInfoPolicy.PRESERVE))
    try:
        frame_mode = FrameComputationMode(str(options_raw.get("frame_mode", FrameComputationMode.PRESERVE.value)))
    except ValueError as exc:
        raise ValueError("frame_mode must be one of: preserve, recompute") from exc

    rules_raw = plan.get("rules", [])
    if not isinstance(rules_raw, list):
        raise ValueError("patch plan rules must be a list")

    return {
        "options": PatchOptions(debug_info=debug_info, frame_mode=frame_mode),
        "rules": [_require_object(rule, "rule") for rule in rules_raw],
    }


def _compile_rule(index: int, rule: dict[str, object]) -> CompiledRule:
    kind = str(rule.get("kind", ""))
    if kind not in {"class", "field", "method"}:
        raise ValueError(f"unsupported rule kind {kind!r}")

    name = str(rule.get("name") or f"rule-{index + 1}")
    owner = _require_object(rule.get("owner", {}), f"{name} owner")
    matcher = _require_object(rule.get("matcher", {}), f"{name} matcher")
    action = rule.get("action")
    action_obj = None if action is None else _require_object(action, f"{name} action")

    code_actions: list[CompiledCodeAction] = []
    if kind == "method":
        raw_code_actions = rule.get("code_actions", [])
        if not isinstance(raw_code_actions, list):
            raise ValueError(f"{name} code_actions must be a list")
        for code_action in raw_code_actions:
            code_actions.extend(_compile_code_action(_require_object(code_action, f"{name} code action")))

    return CompiledRule(
        name=name,
        kind=kind,
        owner=owner,
        matcher=matcher,
        action=action_obj,
        code_actions=code_actions,
    )


def _compile_code_action(action: dict[str, object]) -> list[CompiledCodeAction]:
    action_type = _require_str(action, "type")

    if action_type == "replace-insn":
        matcher = _build_insn_matcher(_require_object(action.get("matcher"), "replace-insn matcher"))
        replacement = _build_replacement_items(_require_list(action.get("replacement"), "replace-insn replacement"))
        return [CompiledCodeAction(transform=CodeTransform.replace_insn(matcher, replacement), matcher=matcher)]

    if action_type == "remove-insn":
        opcode = _require_int(action, "opcode")
        matcher = InsnMatcher.opcode(opcode)
        return [CompiledCodeAction(transform=CodeTransform.remove_insn(matcher), matcher=matcher)]

    if action_type == "insert-before":
        matcher = _build_insn_matcher(_require_object(action.get("matcher"), "insert-before matcher"))
        items = _build_replacement_items(_require_list(action.get("items"), "insert-before items"))
        if not items:
            raise ValueError("insert-before items must not be empty")
        return [CompiledCodeAction(transform=CodeTransform.insert_before(matcher, items), matcher=matcher)]

    if action_type == "insert-after":
        matcher = _build_insn_matcher(_require_object(action.get("matcher"), "insert-after matcher"))
        items = _build_replacement_items(_require_list(action.get("items"), "insert-after items"))
        if not items:
            raise ValueError("insert-after items must not be empty")
        return [CompiledCodeAction(transform=CodeTransform.insert_after(matcher, items), matcher=matcher)]

    if action_type == "remove-sequence":
        pattern = _build_pattern(_require_list(action.get("pattern"), "remove-sequence pattern"))
        if not pattern:
            raise ValueError("remove-sequence pattern must not be empty")
        return [CompiledCodeAction(transform=CodeTransform.remove_sequence(pattern), pattern=pattern)]

    if action_type == "replace-sequence":
        pattern = _build_pattern(_require_list(action.get("pattern"), "replace-sequence pattern"))
        if not pattern:
            raise ValueError("replace-sequence pattern must not be empty")
        replacement = _build_replacement_items(_require_list(action.get("replacement"), "replace-sequence replacement"))
        return [CompiledCodeAction(transform=CodeTransform.replace_sequence(pattern, replacement), pattern=pattern)]

    if action_type == "redirect-method-call":
        return [
            CompiledCodeAction(
                transform=CodeTransform.redirect_method_call(
                    from_owner=_require_str(action, "from_owner"),
                    from_name=_require_str(action, "from_name"),
                    from_descriptor=_require_str(action, "from_descriptor"),
                    to_owner=_require_str(action, "to_owner"),
                    to_name=_require_str(action, "to_name"),
                    to_descriptor=_optional_str(action.get("to_descriptor")),
                ),
                matcher=(
                    InsnMatcher.method_owner(_require_str(action, "from_owner"))
                    & InsnMatcher.method_named(_require_str(action, "from_name"))
                    & InsnMatcher.method_descriptor(_require_str(action, "from_descriptor"))
                ),
            )
        ]

    if action_type == "redirect-field-access":
        return [
            CompiledCodeAction(
                transform=CodeTransform.redirect_field_access(
                    from_owner=_require_str(action, "from_owner"),
                    from_name=_require_str(action, "from_name"),
                    to_owner=_require_str(action, "to_owner"),
                    to_name=_require_str(action, "to_name"),
                ),
                matcher=(
                    InsnMatcher.is_field_access()
                    & InsnMatcher.field_owner(_require_str(action, "from_owner"))
                    & InsnMatcher.field_named(_require_str(action, "from_name"))
                ),
            )
        ]

    if action_type == "replace-string":
        from_value = _require_str(action, "from")
        matcher = InsnMatcher.ldc_string(from_value)
        return [
            CompiledCodeAction(
                transform=CodeTransform.replace_string(from_value, _require_str(action, "to")),
                matcher=matcher,
            )
        ]

    if action_type == "sequence":
        compiled: list[CompiledCodeAction] = []
        for nested in _require_list(action.get("actions"), "sequence actions"):
            compiled.extend(_compile_code_action(_require_object(nested, "sequence action")))
        return compiled

    raise ValueError(f"unsupported code action type {action_type!r}")


def _build_pattern(pattern: list[object]) -> list[InsnMatcher]:
    return [_build_insn_matcher(_require_object(item, "code pattern matcher")) for item in pattern]


def _build_insn_matcher(config: dict[str, object]) -> InsnMatcher:
    matchers: list[InsnMatcher] = []

    if "opcode" in config:
        matchers.append(InsnMatcher.opcode(_require_int(config, "opcode")))
    if "opcode_any" in config:
        matchers.append(InsnMatcher.opcode_any(_int_list(config.get("opcode_any"), "opcode_any")))
    if _bool(config.get("is_field_access")):
        matchers.append(InsnMatcher.is_field_access())
    if _bool(config.get("is_method_call")):
        matchers.append(InsnMatcher.is_method_call())
    if _bool(config.get("is_return")):
        matchers.append(InsnMatcher.is_return())
    if _bool(config.get("is_branch")):
        matchers.append(InsnMatcher.is_branch())
    if _bool(config.get("is_label")):
        matchers.append(InsnMatcher.is_label())
    if _bool(config.get("is_ldc")):
        matchers.append(InsnMatcher.is_ldc())
    if _bool(config.get("is_var")):
        matchers.append(InsnMatcher.is_var())
    if "field_owner" in config:
        matchers.append(InsnMatcher.field_owner(_require_str(config, "field_owner")))
    if "field_name" in config:
        matchers.append(InsnMatcher.field_named(_require_str(config, "field_name")))
    if "field_descriptor" in config:
        matchers.append(InsnMatcher.field_descriptor(_require_str(config, "field_descriptor")))
    if "method_owner" in config:
        matchers.append(InsnMatcher.method_owner(_require_str(config, "method_owner")))
    if "method_name" in config:
        matchers.append(InsnMatcher.method_named(_require_str(config, "method_name")))
    if "method_descriptor" in config:
        matchers.append(InsnMatcher.method_descriptor(_require_str(config, "method_descriptor")))
    if "type_descriptor" in config:
        matchers.append(InsnMatcher.type_descriptor(_require_str(config, "type_descriptor")))
    if "ldc_string" in config:
        matchers.append(InsnMatcher.ldc_string(_require_str(config, "ldc_string")))
    if "var_slot" in config:
        matchers.append(InsnMatcher.var_slot(_require_int(config, "var_slot")))
    if "field_owner_matches" in config:
        matchers.append(InsnMatcher.field_owner_matches(_require_str(config, "field_owner_matches")))
    if "method_owner_matches" in config:
        matchers.append(InsnMatcher.method_owner_matches(_require_str(config, "method_owner_matches")))
    if "method_name_matches" in config:
        matchers.append(InsnMatcher.method_name_matches(_require_str(config, "method_name_matches")))

    matcher = InsnMatcher.any()
    for entry in matchers:
        matcher = matcher & entry
    return matcher


def _build_replacement_items(items: list[object]) -> list[object]:
    labels: dict[str, Label] = {}
    typed_items: list[object] = []

    for raw_item in items:
        item = _require_object(raw_item, "replacement item")
        if item.get("type") == "label":
            name = _require_str(item, "name")
            if name in labels:
                raise ValueError(f"duplicate replacement label {name!r}")
            labels[name] = Label.named(name)

    for raw_item in items:
        item = _require_object(raw_item, "replacement item")
        item_type = _require_str(item, "type")
        if item_type == "raw":
            typed_items.append(RawInsn(_require_int(item, "opcode")))
        elif item_type == "label":
            typed_items.append(labels[_require_str(item, "name")])
        elif item_type == "ldc-string":
            typed_items.append(LdcInsn.string(_require_str(item, "value")))
        elif item_type == "field":
            typed_items.append(
                FieldInsn(
                    _require_int(item, "opcode"),
                    _require_str(item, "owner"),
                    _require_str(item, "name"),
                    _require_str(item, "descriptor"),
                )
            )
        elif item_type == "method":
            typed_items.append(
                MethodInsn(
                    _require_int(item, "opcode"),
                    _require_str(item, "owner"),
                    _require_str(item, "name"),
                    _require_str(item, "descriptor"),
                    bool(item.get("is_interface", False)),
                )
            )
        elif item_type == "type":
            typed_items.append(TypeInsn(_require_int(item, "opcode"), _require_str(item, "descriptor")))
        elif item_type == "var":
            typed_items.append(VarInsn(_require_int(item, "opcode"), _require_int(item, "slot")))
        elif item_type == "iinc":
            typed_items.append(IIncInsn(_require_int(item, "slot"), _require_int(item, "value")))
        elif item_type == "branch":
            typed_items.append(
                BranchInsn(
                    _require_int(item, "opcode"),
                    _lookup_label(labels, _require_str(item, "target")),
                )
            )
        elif item_type == "lookupswitch":
            pairs = []
            for raw_pair in _require_list(item.get("pairs"), "lookupswitch pairs"):
                pair = _require_object(raw_pair, "lookupswitch pair")
                pairs.append((_require_int(pair, "key"), _lookup_label(labels, _require_str(pair, "target"))))
            typed_items.append(
                LookupSwitchInsn(
                    _lookup_label(labels, _require_str(item, "default_target")),
                    pairs,
                )
            )
        elif item_type == "tableswitch":
            low = _require_int(item, "low")
            high = _require_int(item, "high")
            targets = [
                _lookup_label(labels, target) for target in _str_list(item.get("targets"), "tableswitch targets")
            ]
            expected = high - low + 1
            if expected < 0 or len(targets) != expected:
                raise ValueError(f"table-switch target count {len(targets)} does not match range {low}..={high}")
            typed_items.append(
                TableSwitchInsn(
                    _lookup_label(labels, _require_str(item, "default_target")),
                    low,
                    high,
                    targets,
                )
            )
        else:
            raise ValueError(f"unsupported replacement item type {item_type!r}")

    return typed_items


def _lookup_label(labels: dict[str, Label], name: str) -> Label:
    try:
        return labels[name]
    except KeyError as exc:
        raise ValueError(f"replacement branch target {name!r} was not declared") from exc


def _apply_rule(model: ClassModel, rule: CompiledRule) -> tuple[int, int]:
    if rule.kind == "class":
        return _apply_class_rule(model, rule)
    if rule.kind == "field":
        return _apply_field_rule(model, rule)
    return _apply_method_rule(model, rule)


def _apply_class_rule(model: ClassModel, rule: CompiledRule) -> tuple[int, int]:
    if not _class_matches(model, rule.matcher):
        return 0, 0
    changed = _apply_class_action(model, _require_object(rule.action, f"{rule.name} action"))
    return 1, int(changed)


def _apply_field_rule(model: ClassModel, rule: CompiledRule) -> tuple[int, int]:
    if not _class_matches(model, rule.owner):
        return 0, 0
    return _apply_field_action(model, rule.matcher, _require_object(rule.action, f"{rule.name} action"))


def _apply_method_rule(model: ClassModel, rule: CompiledRule) -> tuple[int, int]:
    if not _class_matches(model, rule.owner):
        return 0, 0

    matched_targets = 0
    changed_targets = 0

    if rule.action is not None:
        matched, changed = _apply_method_action(model, rule.matcher, rule.action)
        matched_targets += matched
        changed_targets += changed

    for action in rule.code_actions:
        matched, changed = _apply_code_action(model, rule.matcher, action)
        matched_targets += matched
        changed_targets += changed

    return matched_targets, changed_targets


def _apply_class_action(model: ClassModel, action: dict[str, object]) -> bool:
    action_type = _require_str(action, "type")
    if action_type == "add-access-flags":
        flags = _flag_mask(_require_list(action.get("flags"), "class access flags"), ClassAccessFlag)
        changed = model.access_flags & flags != flags
        model.access_flags |= flags
        return changed
    if action_type == "remove-access-flags":
        flags = _flag_mask(_require_list(action.get("flags"), "class access flags"), ClassAccessFlag)
        changed = model.access_flags & flags != 0
        model.access_flags &= ~flags
        return changed
    if action_type == "set-access-flags":
        flags = _flag_mask(_require_list(action.get("flags"), "class access flags"), ClassAccessFlag)
        changed = model.access_flags != flags
        model.access_flags = flags
        return changed
    if action_type == "set-super-class":
        target = _optional_str(action.get("to"))
        changed = model.super_name != target
        model.super_name = target
        return changed
    if action_type == "add-interface":
        name = _require_str(action, "name")
        interfaces = list(model.interfaces)
        if name in interfaces:
            return False
        interfaces.append(name)
        model.interfaces = interfaces
        return True
    if action_type == "remove-interface":
        name = _require_str(action, "name")
        interfaces = [entry for entry in model.interfaces if entry != name]
        changed = len(interfaces) != len(model.interfaces)
        model.interfaces = interfaces
        return changed
    raise ValueError(f"unsupported class action type {action_type!r}")


def _apply_field_action(
    model: ClassModel,
    matcher: dict[str, object],
    action: dict[str, object],
) -> tuple[int, int]:
    action_type = _require_str(action, "type")
    fields = list(model.fields)

    if action_type == "remove":
        retained = [field for field in fields if not _field_matches(field, matcher)]
        matched = len(fields) - len(retained)
        if matched:
            model.fields = retained
        return matched, matched

    matched = 0
    changed = 0
    for field in fields:
        if not _field_matches(field, matcher):
            continue
        matched += 1
        if action_type == "rename":
            target = _require_str(action, "to")
            if field.name != target:
                field.name = target
                changed += 1
        elif action_type == "add-access-flags":
            flags = _flag_mask(_require_list(action.get("flags"), "field access flags"), FieldAccessFlag)
            if field.access_flags & flags != flags:
                field.access_flags |= flags
                changed += 1
        elif action_type == "remove-access-flags":
            flags = _flag_mask(_require_list(action.get("flags"), "field access flags"), FieldAccessFlag)
            if field.access_flags & flags:
                field.access_flags &= ~flags
                changed += 1
        elif action_type == "set-access-flags":
            flags = _flag_mask(_require_list(action.get("flags"), "field access flags"), FieldAccessFlag)
            if field.access_flags != flags:
                field.access_flags = flags
                changed += 1
        else:
            raise ValueError(f"unsupported field action type {action_type!r}")
    return matched, changed


def _apply_method_action(
    model: ClassModel,
    matcher: dict[str, object],
    action: dict[str, object],
) -> tuple[int, int]:
    action_type = _require_str(action, "type")
    methods = list(model.methods)

    if action_type == "remove":
        retained = [method for method in methods if not _method_matches(method, matcher)]
        matched = len(methods) - len(retained)
        if matched:
            model.methods = retained
        return matched, matched

    matched = 0
    changed = 0
    for method in methods:
        if not _method_matches(method, matcher):
            continue
        matched += 1
        if action_type == "rename":
            target = _require_str(action, "to")
            if method.name != target:
                method.name = target
                changed += 1
        elif action_type == "add-access-flags":
            flags = _flag_mask(_require_list(action.get("flags"), "method access flags"), MethodAccessFlag)
            if method.access_flags & flags != flags:
                method.access_flags |= flags
                changed += 1
        elif action_type == "remove-access-flags":
            flags = _flag_mask(_require_list(action.get("flags"), "method access flags"), MethodAccessFlag)
            if method.access_flags & flags:
                method.access_flags &= ~flags
                changed += 1
        elif action_type == "set-access-flags":
            flags = _flag_mask(_require_list(action.get("flags"), "method access flags"), MethodAccessFlag)
            if method.access_flags != flags:
                method.access_flags = flags
                changed += 1
        else:
            raise ValueError(f"unsupported method action type {action_type!r}")
    return matched, changed


def _apply_code_action(
    model: ClassModel,
    matcher: dict[str, object],
    action: CompiledCodeAction,
) -> tuple[int, int]:
    matched = 0
    changed = 0

    for method in list(model.methods):
        if not _method_matches(method, matcher):
            continue
        code = method.code
        if code is None:
            continue

        if action.matcher is not None:
            target_matches = code.count_insns(action.matcher)
        else:
            assert action.pattern is not None
            target_matches = len(code.find_sequences(action.pattern))

        if target_matches == 0:
            continue

        PipelineBuilder().on_code(
            MethodMatcher.named(method.name) & MethodMatcher.descriptor(method.descriptor),
            action.transform,
            owner_matcher=class_named(model.name),
        ).apply(model)
        matched += target_matches
        changed += target_matches

    return matched, changed


def _class_matches(model: ClassModel, matcher: dict[str, object]) -> bool:
    if not matcher:
        return True
    if "name" in matcher and model.name != _require_str(matcher, "name"):
        return False
    if "name_matches" in matcher and not _regex(matcher, "name_matches").search(model.name):
        return False
    if "access_all" in matcher:
        flags = _flag_mask(_require_list(matcher.get("access_all"), "class access_all"), ClassAccessFlag)
        if model.access_flags & flags != flags:
            return False
    if "access_any" in matcher:
        flags = _flag_mask(_require_list(matcher.get("access_any"), "class access_any"), ClassAccessFlag)
        if flags and model.access_flags & flags == 0:
            return False
    if "package_private" in matcher:
        is_package_private = model.access_flags & CLASS_VISIBILITY_FLAGS == 0
        if is_package_private != _bool(matcher.get("package_private")):
            return False
    if "extends" in matcher and model.super_name != _require_str(matcher, "extends"):
        return False
    for interface_name in _str_list(matcher.get("implements"), "implements"):
        if interface_name not in model.interfaces:
            return False
    major_version = int(model.version[0])
    if "version" in matcher and major_version != _require_int(matcher, "version"):
        return False
    if "version_at_least" in matcher and major_version < _require_int(matcher, "version_at_least"):
        return False
    if "version_below" in matcher and major_version >= _require_int(matcher, "version_below"):
        return False
    return True


def _field_matches(field: object, matcher: dict[str, object]) -> bool:
    if "name" in matcher and getattr(field, "name") != _require_str(matcher, "name"):
        return False
    if "name_matches" in matcher and not _regex(matcher, "name_matches").search(str(getattr(field, "name"))):
        return False
    if "descriptor" in matcher and getattr(field, "descriptor") != _require_str(matcher, "descriptor"):
        return False
    if "descriptor_matches" in matcher and not _regex(matcher, "descriptor_matches").search(
        str(getattr(field, "descriptor"))
    ):
        return False
    if "access_all" in matcher:
        flags = _flag_mask(_require_list(matcher.get("access_all"), "field access_all"), FieldAccessFlag)
        if int(getattr(field, "access_flags")) & flags != flags:
            return False
    if "access_any" in matcher:
        flags = _flag_mask(_require_list(matcher.get("access_any"), "field access_any"), FieldAccessFlag)
        if flags and int(getattr(field, "access_flags")) & flags == 0:
            return False
    if "package_private" in matcher:
        is_package_private = int(getattr(field, "access_flags")) & MEMBER_VISIBILITY_FLAGS == 0
        if is_package_private != _bool(matcher.get("package_private")):
            return False
    return True


def _method_matches(method: object, matcher: dict[str, object]) -> bool:
    if "name" in matcher and getattr(method, "name") != _require_str(matcher, "name"):
        return False
    if "name_matches" in matcher and not _regex(matcher, "name_matches").search(str(getattr(method, "name"))):
        return False
    if "descriptor" in matcher and getattr(method, "descriptor") != _require_str(matcher, "descriptor"):
        return False
    if "descriptor_matches" in matcher and not _regex(matcher, "descriptor_matches").search(
        str(getattr(method, "descriptor"))
    ):
        return False
    if "access_all" in matcher:
        flags = _flag_mask(_require_list(matcher.get("access_all"), "method access_all"), MethodAccessFlag)
        if int(getattr(method, "access_flags")) & flags != flags:
            return False
    if "access_any" in matcher:
        flags = _flag_mask(_require_list(matcher.get("access_any"), "method access_any"), MethodAccessFlag)
        if flags and int(getattr(method, "access_flags")) & flags == 0:
            return False
    if "package_private" in matcher:
        is_package_private = int(getattr(method, "access_flags")) & MEMBER_VISIBILITY_FLAGS == 0
        if is_package_private != _bool(matcher.get("package_private")):
            return False
    if "has_code" in matcher and (getattr(method, "code") is not None) != _bool(matcher.get("has_code")):
        return False
    return True


def _flag_mask(raw_flags: list[object], enum_type: type[IntFlag]) -> int:
    mask = enum_type(0)
    for raw_flag in raw_flags:
        if not isinstance(raw_flag, str):
            raise ValueError("access flags must be strings")
        flag = raw_flag.upper()
        try:
            mask |= enum_type[flag]
        except KeyError as exc:
            raise ValueError(f"unsupported access flag {raw_flag!r}") from exc
    return int(mask)


def _regex(config: dict[str, object], key: str) -> Pattern[str]:
    cache_key = f"__compiled_{key}"
    cached = config.get(cache_key)
    if isinstance(cached, re.Pattern):
        return cached
    compiled = re.compile(_require_str(config, key))
    config[cache_key] = compiled
    return compiled


def _require_object(value: object, label: str) -> dict[str, object]:
    if not isinstance(value, dict):
        raise ValueError(f"{label} must be an object")
    return value


def _require_list(value: object, label: str) -> list[object]:
    if not isinstance(value, list):
        raise ValueError(f"{label} must be a list")
    return value


def _require_str(mapping: dict[str, object], key: str) -> str:
    value = mapping.get(key)
    if not isinstance(value, str):
        raise ValueError(f"{key} must be a string")
    return value


def _optional_str(value: object) -> str | None:
    if value is None:
        return None
    if not isinstance(value, str):
        raise ValueError("optional string value must be a string or null")
    return value


def _require_int(mapping: dict[str, object], key: str) -> int:
    value = mapping.get(key)
    if not isinstance(value, int):
        raise ValueError(f"{key} must be an integer")
    return value


def _int_list(value: object, label: str) -> list[int]:
    values = _require_list(value, label)
    result: list[int] = []
    for entry in values:
        if not isinstance(entry, int):
            raise ValueError(f"{label} entries must be integers")
        result.append(entry)
    return result


def _str_list(value: object, label: str) -> list[str]:
    if value is None:
        return []
    values = _require_list(value, label)
    result: list[str] = []
    for entry in values:
        if not isinstance(entry, str):
            raise ValueError(f"{label} entries must be strings")
        result.append(entry)
    return result


def _bool(value: object) -> bool:
    if not isinstance(value, bool):
        raise ValueError("boolean field must be true or false")
    return value


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Apply patch-jar style JSON rules through pytecode's Python API.")
    parser.add_argument("--jar", type=Path, required=True, help="Input JAR to rewrite.")
    parser.add_argument("--output", type=Path, required=True, help="Output path for the rewritten JAR.")
    parser.add_argument("--rules", type=Path, required=True, help="Path to the JSON patch rules file.")
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = _build_parser().parse_args(argv)
    report = patch_jar(args.jar, args.output, args.rules)
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
