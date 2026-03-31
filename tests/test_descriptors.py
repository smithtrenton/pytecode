"""Tests for pytecode.descriptors — descriptor and signature parsing utilities."""

from __future__ import annotations

import pytest

from pytecode.descriptors import (
    VOID,
    ArrayType,
    ArrayTypeSignature,
    BaseType,
    ClassSignature,
    ClassTypeSignature,
    MethodDescriptor,
    MethodSignature,
    ObjectType,
    TypeArgument,
    TypeVariable,
    VoidType,
    is_valid_field_descriptor,
    is_valid_method_descriptor,
    parameter_slot_count,
    parse_class_signature,
    parse_field_descriptor,
    parse_field_signature,
    parse_method_descriptor,
    parse_method_signature,
    slot_size,
    to_descriptor,
)

# ===================================================================
# Field descriptor parsing
# ===================================================================


class TestParseFieldDescriptor:
    """parse_field_descriptor()"""

    @pytest.mark.parametrize(
        ("descriptor", "expected"),
        [
            ("Z", BaseType.BOOLEAN),
            ("B", BaseType.BYTE),
            ("C", BaseType.CHAR),
            ("S", BaseType.SHORT),
            ("I", BaseType.INT),
            ("J", BaseType.LONG),
            ("F", BaseType.FLOAT),
            ("D", BaseType.DOUBLE),
        ],
    )
    def test_base_types(self, descriptor: str, expected: BaseType) -> None:
        assert parse_field_descriptor(descriptor) is expected

    def test_object_type(self) -> None:
        result = parse_field_descriptor("Ljava/lang/String;")
        assert result == ObjectType("java/lang/String")

    def test_object_type_default_package(self) -> None:
        result = parse_field_descriptor("LFoo;")
        assert result == ObjectType("Foo")

    def test_array_of_int(self) -> None:
        result = parse_field_descriptor("[I")
        assert result == ArrayType(BaseType.INT)

    def test_array_of_object(self) -> None:
        result = parse_field_descriptor("[Ljava/lang/Object;")
        assert result == ArrayType(ObjectType("java/lang/Object"))

    def test_nested_array(self) -> None:
        result = parse_field_descriptor("[[I")
        assert result == ArrayType(ArrayType(BaseType.INT))

    def test_deeply_nested_array(self) -> None:
        result = parse_field_descriptor("[[[D")
        assert result == ArrayType(ArrayType(ArrayType(BaseType.DOUBLE)))

    def test_array_of_array_of_object(self) -> None:
        result = parse_field_descriptor("[[Ljava/lang/String;")
        assert result == ArrayType(ArrayType(ObjectType("java/lang/String")))

    def test_trailing_chars_rejected(self) -> None:
        with pytest.raises(ValueError, match="trailing characters"):
            parse_field_descriptor("IX")

    def test_empty_string_rejected(self) -> None:
        with pytest.raises(ValueError, match="unexpected end"):
            parse_field_descriptor("")

    def test_void_rejected(self) -> None:
        with pytest.raises(ValueError, match="invalid descriptor character"):
            parse_field_descriptor("V")

    def test_invalid_char_rejected(self) -> None:
        with pytest.raises(ValueError, match="invalid descriptor character"):
            parse_field_descriptor("X")

    def test_unclosed_object_type(self) -> None:
        with pytest.raises(ValueError, match="unexpected end"):
            parse_field_descriptor("Ljava/lang/String")

    def test_empty_object_type(self) -> None:
        with pytest.raises(ValueError, match="empty class name"):
            parse_field_descriptor("L;")

    def test_object_type_rejects_dotted_internal_name(self) -> None:
        with pytest.raises(ValueError, match="invalid character '\\.' in class name"):
            parse_field_descriptor("Ljava.lang.String;")

    def test_object_type_rejects_empty_path_segments(self) -> None:
        with pytest.raises(ValueError, match="empty class name segment"):
            parse_field_descriptor("Ljava//lang/String;")

    def test_array_without_component(self) -> None:
        with pytest.raises(ValueError, match="unexpected end"):
            parse_field_descriptor("[")


# ===================================================================
# Method descriptor parsing
# ===================================================================


class TestParseMethodDescriptor:
    """parse_method_descriptor()"""

    def test_no_params_void(self) -> None:
        result = parse_method_descriptor("()V")
        assert result == MethodDescriptor((), VOID)

    def test_no_params_returns_int(self) -> None:
        result = parse_method_descriptor("()I")
        assert result == MethodDescriptor((), BaseType.INT)

    def test_single_int_param(self) -> None:
        result = parse_method_descriptor("(I)V")
        assert result == MethodDescriptor((BaseType.INT,), VOID)

    def test_multiple_params(self) -> None:
        result = parse_method_descriptor("(IDLjava/lang/Thread;)Ljava/lang/Object;")
        assert result == MethodDescriptor(
            (BaseType.INT, BaseType.DOUBLE, ObjectType("java/lang/Thread")),
            ObjectType("java/lang/Object"),
        )

    def test_array_params(self) -> None:
        result = parse_method_descriptor("([I[Ljava/lang/String;)V")
        assert result == MethodDescriptor(
            (ArrayType(BaseType.INT), ArrayType(ObjectType("java/lang/String"))),
            VOID,
        )

    def test_returns_array(self) -> None:
        result = parse_method_descriptor("()[Ljava/lang/String;")
        assert result == MethodDescriptor((), ArrayType(ObjectType("java/lang/String")))

    def test_all_base_types(self) -> None:
        result = parse_method_descriptor("(ZBCSIJFD)V")
        assert result == MethodDescriptor(
            (
                BaseType.BOOLEAN,
                BaseType.BYTE,
                BaseType.CHAR,
                BaseType.SHORT,
                BaseType.INT,
                BaseType.LONG,
                BaseType.FLOAT,
                BaseType.DOUBLE,
            ),
            VOID,
        )

    def test_missing_open_paren(self) -> None:
        with pytest.raises(ValueError, match="expected '\\('"):
            parse_method_descriptor("I)V")

    def test_missing_close_paren(self) -> None:
        with pytest.raises(ValueError, match="unexpected end"):
            parse_method_descriptor("(I")

    def test_trailing_chars(self) -> None:
        with pytest.raises(ValueError, match="trailing characters"):
            parse_method_descriptor("()VX")

    def test_empty_string(self) -> None:
        with pytest.raises(ValueError, match="unexpected end"):
            parse_method_descriptor("")


# ===================================================================
# Descriptor construction (round-trip)
# ===================================================================


class TestToDescriptor:
    """to_descriptor()"""

    @pytest.mark.parametrize(
        ("type_", "expected"),
        [
            (BaseType.INT, "I"),
            (BaseType.LONG, "J"),
            (BaseType.BOOLEAN, "Z"),
            (ObjectType("java/lang/String"), "Ljava/lang/String;"),
            (ArrayType(BaseType.INT), "[I"),
            (ArrayType(ArrayType(BaseType.DOUBLE)), "[[D"),
            (ArrayType(ObjectType("java/lang/Object")), "[Ljava/lang/Object;"),
        ],
    )
    def test_field_descriptor(self, type_: BaseType | ObjectType | ArrayType, expected: str) -> None:
        assert to_descriptor(type_) == expected

    def test_method_no_params_void(self) -> None:
        assert to_descriptor(MethodDescriptor((), VOID)) == "()V"

    def test_method_with_params(self) -> None:
        md = MethodDescriptor(
            (BaseType.INT, BaseType.DOUBLE, ObjectType("java/lang/Thread")),
            ObjectType("java/lang/Object"),
        )
        assert to_descriptor(md) == "(IDLjava/lang/Thread;)Ljava/lang/Object;"

    def test_method_returns_array(self) -> None:
        md = MethodDescriptor((), ArrayType(BaseType.BYTE))
        assert to_descriptor(md) == "()[B"


class TestRoundTrip:
    """Verify parse → to_descriptor → parse is identity."""

    @pytest.mark.parametrize(
        "descriptor",
        [
            "I",
            "J",
            "Ljava/lang/String;",
            "[I",
            "[[Ljava/lang/Object;",
            "[[[Z",
        ],
    )
    def test_field_descriptor_round_trip(self, descriptor: str) -> None:
        parsed = parse_field_descriptor(descriptor)
        assert to_descriptor(parsed) == descriptor

    @pytest.mark.parametrize(
        "descriptor",
        [
            "()V",
            "(I)V",
            "(IDLjava/lang/Thread;)Ljava/lang/Object;",
            "([I[Ljava/lang/String;)V",
            "()[Ljava/lang/String;",
            "(ZBCSIJFD)V",
        ],
    )
    def test_method_descriptor_round_trip(self, descriptor: str) -> None:
        parsed = parse_method_descriptor(descriptor)
        assert to_descriptor(parsed) == descriptor


# ===================================================================
# Slot helpers
# ===================================================================


class TestSlotSize:
    """slot_size()"""

    @pytest.mark.parametrize(
        ("type_", "expected"),
        [
            (BaseType.LONG, 2),
            (BaseType.DOUBLE, 2),
            (BaseType.INT, 1),
            (BaseType.BOOLEAN, 1),
            (BaseType.BYTE, 1),
            (BaseType.CHAR, 1),
            (BaseType.SHORT, 1),
            (BaseType.FLOAT, 1),
            (ObjectType("java/lang/Object"), 1),
            (ArrayType(BaseType.INT), 1),
        ],
    )
    def test_slot_size(self, type_: BaseType | ObjectType | ArrayType, expected: int) -> None:
        assert slot_size(type_) == expected


class TestParameterSlotCount:
    """parameter_slot_count()"""

    def test_no_params(self) -> None:
        assert parameter_slot_count(MethodDescriptor((), VOID)) == 0

    def test_single_int(self) -> None:
        assert parameter_slot_count(MethodDescriptor((BaseType.INT,), VOID)) == 1

    def test_long_double(self) -> None:
        md = MethodDescriptor((BaseType.LONG, BaseType.DOUBLE), VOID)
        assert parameter_slot_count(md) == 4

    def test_mixed(self) -> None:
        md = MethodDescriptor(
            (BaseType.INT, BaseType.LONG, ObjectType("Foo"), BaseType.DOUBLE, BaseType.BYTE),
            VOID,
        )
        # 1 + 2 + 1 + 2 + 1 = 7
        assert parameter_slot_count(md) == 7


# ===================================================================
# Validation helpers
# ===================================================================


class TestValidation:
    """is_valid_field_descriptor() / is_valid_method_descriptor()"""

    @pytest.mark.parametrize(
        "descriptor",
        ["I", "J", "Ljava/lang/String;", "[I", "[[D", "[Ljava/lang/Object;"],
    )
    def test_valid_field_descriptors(self, descriptor: str) -> None:
        assert is_valid_field_descriptor(descriptor) is True

    @pytest.mark.parametrize(
        "descriptor",
        ["", "V", "X", "L;", "[", "Ljava/lang/String", "II"],
    )
    def test_invalid_field_descriptors(self, descriptor: str) -> None:
        assert is_valid_field_descriptor(descriptor) is False

    @pytest.mark.parametrize(
        "descriptor",
        ["()V", "(I)V", "(IDLjava/lang/Thread;)Ljava/lang/Object;", "()[I"],
    )
    def test_valid_method_descriptors(self, descriptor: str) -> None:
        assert is_valid_method_descriptor(descriptor) is True

    @pytest.mark.parametrize(
        "descriptor",
        ["", "V", "I)V", "(I", "()VX", "(V)V"],
    )
    def test_invalid_method_descriptors(self, descriptor: str) -> None:
        assert is_valid_method_descriptor(descriptor) is False


# ===================================================================
# Generic signature parsing — field signatures
# ===================================================================


class TestParseFieldSignature:
    """parse_field_signature()"""

    def test_simple_class_type(self) -> None:
        result = parse_field_signature("Ljava/lang/String;")
        assert result == ClassTypeSignature("java/lang/", "String", (), ())

    def test_parameterized_type(self) -> None:
        result = parse_field_signature("Ljava/util/List<Ljava/lang/String;>;")
        assert result == ClassTypeSignature(
            "java/util/",
            "List",
            (TypeArgument(None, ClassTypeSignature("java/lang/", "String", (), ())),),
            (),
        )

    def test_type_variable(self) -> None:
        result = parse_field_signature("TT;")
        assert result == TypeVariable("T")

    def test_array_of_type_variable(self) -> None:
        result = parse_field_signature("[TT;")
        assert result == ArrayTypeSignature(TypeVariable("T"))

    def test_nested_generics(self) -> None:
        # Map<String, List<Integer>>
        result = parse_field_signature("Ljava/util/Map<Ljava/lang/String;Ljava/util/List<Ljava/lang/Integer;>;>;")
        assert isinstance(result, ClassTypeSignature)
        assert result.name == "Map"
        assert len(result.type_arguments) == 2
        second_arg = result.type_arguments[1]
        assert second_arg.signature is not None
        assert isinstance(second_arg.signature, ClassTypeSignature)
        assert second_arg.signature.name == "List"

    def test_wildcard_extends(self) -> None:
        # List<? extends Number>
        result = parse_field_signature("Ljava/util/List<+Ljava/lang/Number;>;")
        assert isinstance(result, ClassTypeSignature)
        arg = result.type_arguments[0]
        assert arg.wildcard == "+"
        assert arg.signature == ClassTypeSignature("java/lang/", "Number", (), ())

    def test_wildcard_super(self) -> None:
        # List<? super Integer>
        result = parse_field_signature("Ljava/util/List<-Ljava/lang/Integer;>;")
        assert isinstance(result, ClassTypeSignature)
        arg = result.type_arguments[0]
        assert arg.wildcard == "-"

    def test_unbounded_wildcard(self) -> None:
        # List<?>
        result = parse_field_signature("Ljava/util/List<*>;")
        assert isinstance(result, ClassTypeSignature)
        arg = result.type_arguments[0]
        assert arg.wildcard is None
        assert arg.signature is None

    def test_inner_class(self) -> None:
        # Map<K,V>.Entry<K,V>
        result = parse_field_signature("Ljava/util/Map<TK;TV;>.Entry<TK;TV;>;")
        assert isinstance(result, ClassTypeSignature)
        assert result.name == "Map"
        assert len(result.type_arguments) == 2
        assert len(result.inner) == 1
        entry = result.inner[0]
        assert entry.name == "Entry"
        assert len(entry.type_arguments) == 2
        assert entry.type_arguments[0] == TypeArgument(None, TypeVariable("K"))

    def test_default_package_class(self) -> None:
        result = parse_field_signature("LFoo;")
        assert result == ClassTypeSignature("", "Foo", (), ())

    def test_trailing_chars_rejected(self) -> None:
        with pytest.raises(ValueError, match="trailing characters"):
            parse_field_signature("Ljava/lang/String;X")

    def test_array_of_parameterized(self) -> None:
        # List<String>[]
        result = parse_field_signature("[Ljava/util/List<Ljava/lang/String;>;")
        assert isinstance(result, ArrayTypeSignature)
        assert isinstance(result.component, ClassTypeSignature)
        assert result.component.name == "List"

    def test_rejects_empty_class_name_segments(self) -> None:
        with pytest.raises(ValueError, match="empty class name segment"):
            parse_field_signature("Ljava//util/List;")

    def test_rejects_empty_inner_class_name(self) -> None:
        with pytest.raises(ValueError, match="empty inner class name"):
            parse_field_signature("LFoo.;")


# ===================================================================
# Generic signature parsing — class signatures
# ===================================================================


class TestParseClassSignature:
    """parse_class_signature()"""

    def test_no_type_params(self) -> None:
        result = parse_class_signature("Ljava/lang/Object;")
        assert result == ClassSignature((), ClassTypeSignature("java/lang/", "Object", (), ()), ())

    def test_single_type_param(self) -> None:
        # class Foo<T> extends Object
        result = parse_class_signature("<T:Ljava/lang/Object;>Ljava/lang/Object;")
        assert len(result.type_parameters) == 1
        tp = result.type_parameters[0]
        assert tp.name == "T"
        assert tp.class_bound == ClassTypeSignature("java/lang/", "Object", (), ())
        assert tp.interface_bounds == ()

    def test_type_param_with_interface_bounds(self) -> None:
        # <T:Ljava/lang/Object;:Ljava/io/Serializable;>
        result = parse_class_signature("<T:Ljava/lang/Object;:Ljava/io/Serializable;>Ljava/lang/Object;")
        tp = result.type_parameters[0]
        assert tp.class_bound == ClassTypeSignature("java/lang/", "Object", (), ())
        assert len(tp.interface_bounds) == 1
        assert tp.interface_bounds[0] == ClassTypeSignature("java/io/", "Serializable", (), ())

    def test_empty_class_bound(self) -> None:
        # <T::Ljava/io/Serializable;> — no class bound, one interface bound
        result = parse_class_signature("<T::Ljava/io/Serializable;>Ljava/lang/Object;")
        tp = result.type_parameters[0]
        assert tp.class_bound is None
        assert len(tp.interface_bounds) == 1

    def test_multiple_type_params(self) -> None:
        # <K:Ljava/lang/Object;V:Ljava/lang/Object;>
        result = parse_class_signature("<K:Ljava/lang/Object;V:Ljava/lang/Object;>Ljava/lang/Object;")
        assert len(result.type_parameters) == 2
        assert result.type_parameters[0].name == "K"
        assert result.type_parameters[1].name == "V"

    def test_super_interfaces(self) -> None:
        # class Foo extends Object implements Serializable, Comparable<Foo>
        result = parse_class_signature("Ljava/lang/Object;Ljava/io/Serializable;Ljava/lang/Comparable<LFoo;>;")
        assert result.type_parameters == ()
        assert result.super_class == ClassTypeSignature("java/lang/", "Object", (), ())
        assert len(result.super_interfaces) == 2
        assert result.super_interfaces[0] == ClassTypeSignature("java/io/", "Serializable", (), ())
        assert result.super_interfaces[1].name == "Comparable"

    def test_parameterized_superclass(self) -> None:
        # class StringList extends ArrayList<String>
        result = parse_class_signature("Ljava/util/ArrayList<Ljava/lang/String;>;")
        assert result.super_class.name == "ArrayList"
        assert len(result.super_class.type_arguments) == 1


# ===================================================================
# Generic signature parsing — method signatures
# ===================================================================


class TestParseMethodSignature:
    """parse_method_signature()"""

    def test_no_type_params_void(self) -> None:
        result = parse_method_signature("()V")
        assert result == MethodSignature((), (), VOID, ())

    def test_generic_identity(self) -> None:
        # <T:Ljava/lang/Object;>(TT;)TT;
        result = parse_method_signature("<T:Ljava/lang/Object;>(TT;)TT;")
        assert len(result.type_parameters) == 1
        assert result.type_parameters[0].name == "T"
        assert len(result.parameter_types) == 1
        assert result.parameter_types[0] == TypeVariable("T")
        assert result.return_type == TypeVariable("T")
        assert result.throws == ()

    def test_throws_signature(self) -> None:
        # <T:Ljava/lang/Object;>(TT;)V^Ljava/io/IOException;
        result = parse_method_signature("<T:Ljava/lang/Object;>(TT;)V^Ljava/io/IOException;")
        assert result.return_type is VOID
        assert len(result.throws) == 1
        assert isinstance(result.throws[0], ClassTypeSignature)
        assert result.throws[0].name == "IOException"

    def test_throws_type_variable(self) -> None:
        # <E:Ljava/lang/Exception;>()V^TE;
        result = parse_method_signature("<E:Ljava/lang/Exception;>()V^TE;")
        assert len(result.throws) == 1
        assert result.throws[0] == TypeVariable("E")

    def test_multiple_throws(self) -> None:
        result = parse_method_signature("()V^Ljava/io/IOException;^Ljava/lang/InterruptedException;")
        assert len(result.throws) == 2

    def test_parameterized_param_and_return(self) -> None:
        # (List<String>) -> Map<String, Integer>
        result = parse_method_signature(
            "(Ljava/util/List<Ljava/lang/String;>;)Ljava/util/Map<Ljava/lang/String;Ljava/lang/Integer;>;"
        )
        assert len(result.parameter_types) == 1
        param = result.parameter_types[0]
        assert isinstance(param, ClassTypeSignature)
        assert param.name == "List"
        ret = result.return_type
        assert isinstance(ret, ClassTypeSignature)
        assert ret.name == "Map"
        assert len(ret.type_arguments) == 2

    def test_base_type_params_in_signature(self) -> None:
        # (int, long) -> double  — base types in generic signature context
        result = parse_method_signature("(IJ)D")
        assert result.parameter_types == (BaseType.INT, BaseType.LONG)
        assert result.return_type is BaseType.DOUBLE

    def test_array_param_in_signature(self) -> None:
        result = parse_method_signature("([TT;)V")
        assert len(result.parameter_types) == 1
        param = result.parameter_types[0]
        assert isinstance(param, ArrayTypeSignature)
        assert param.component == TypeVariable("T")


# ===================================================================
# VoidType sentinel
# ===================================================================


class TestVoidType:
    def test_void_singleton(self) -> None:
        assert VOID is VoidType.VOID

    def test_void_value(self) -> None:
        assert VOID.value == "V"
