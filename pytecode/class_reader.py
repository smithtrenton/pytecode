from functools import partial

from . import attributes, constant_pool, constants, info, instructions
from .bytes_utils import BytesReader


class MalformedClassException(Exception):
    pass


# TODO: Rework the reader to use dataclass annotations for byte reading instead of manual
class ClassReader(BytesReader):
    def __init__(self, bytes_or_bytearray):
        super().__init__(bytes_or_bytearray)
        self.constant_pool = None
        self.read_class()

    @classmethod
    def from_file(cls, path):
        with open(path, "rb") as f:
            file_bytes = f.read()
        return cls(file_bytes)

    @classmethod
    def from_bytes(cls, bytes_or_bytearray):
        return cls(bytes_or_bytearray)

    def read_constant_pool_index(self, index):
        index_extra, offset, tag = 0, self.offset, self.read_u1()
        cp_type = constant_pool.ConstantPoolInfoType(tag)
        cp_class = partial(cp_type.cp_class, index, offset, tag)

        if cp_type in (
            constant_pool.ConstantPoolInfoType.CLASS,
            constant_pool.ConstantPoolInfoType.STRING,
            constant_pool.ConstantPoolInfoType.METHOD_TYPE,
            constant_pool.ConstantPoolInfoType.MODULE,
            constant_pool.ConstantPoolInfoType.PACKAGE,
        ):
            cp_info = cp_class(self.read_u2())
        elif cp_type in (
            constant_pool.ConstantPoolInfoType.FIELD_REF,
            constant_pool.ConstantPoolInfoType.METHOD_REF,
            constant_pool.ConstantPoolInfoType.INTERFACE_METHOD_REF,
            constant_pool.ConstantPoolInfoType.NAME_AND_TYPE,
            constant_pool.ConstantPoolInfoType.DYNAMIC,
            constant_pool.ConstantPoolInfoType.INVOKE_DYNAMIC,
        ):
            cp_info = cp_class(self.read_u2(), self.read_u2())
        elif cp_type in (
            constant_pool.ConstantPoolInfoType.INTEGER,
            constant_pool.ConstantPoolInfoType.FLOAT,
        ):
            cp_info = cp_class(self.read_u4())
        elif cp_type in (
            constant_pool.ConstantPoolInfoType.LONG,
            constant_pool.ConstantPoolInfoType.DOUBLE,
        ):
            cp_info = cp_class(self.read_u4(), self.read_u4())
            index_extra = 1
        elif cp_type is constant_pool.ConstantPoolInfoType.UTF8:
            length = self.read_u2()
            str_bytes = self.read_bytes(length)
            cp_info = cp_class(length, str_bytes)
        elif cp_type is constant_pool.ConstantPoolInfoType.METHOD_HANDLE:
            cp_info = cp_class(self.read_u1(), self.read_u2())
        else:
            raise ValueError("Unknown ConstantPoolInfoType: %s" % cp_type)
        return cp_info, index_extra

    def read_align_bytes(self, current_offset):
        align_bytes = (4 - current_offset % 4) % 4
        return self.read_bytes(align_bytes)

    def read_instruction(self, current_method_offset):
        opcode = self.read_u1()
        inst_type = instructions.InsnInfoType(opcode)
        inst_info = partial(inst_type.instinfo, inst_type, current_method_offset)
        if inst_type.instinfo is instructions.LocalIndex:
            index = self.read_u1()
            return inst_info(index)
        elif inst_type.instinfo is instructions.ConstPoolIndex:
            index = self.read_u2()
            return inst_info(index)
        elif inst_type.instinfo is instructions.ByteValue:
            value = self.read_i1()
            return inst_info(value)
        elif inst_type.instinfo is instructions.ShortValue:
            value = self.read_i2()
            return inst_info(value)
        elif inst_type.instinfo is instructions.Branch:
            offset = self.read_i2()
            return inst_info(offset)
        elif inst_type.instinfo is instructions.BranchW:
            offset = self.read_i4()
            return inst_info(offset)
        elif inst_type.instinfo is instructions.IInc:
            index, value = self.read_u1(), self.read_i1()
            return inst_info(index, value)
        elif inst_type.instinfo is instructions.InvokeDynamic:
            index, unused = self.read_u2(), self.read_bytes(2)
            return inst_info(index, unused)
        elif inst_type.instinfo is instructions.InvokeInterface:
            index, count, unused = self.read_u2(), self.read_u1(), self.read_bytes(1)
            return inst_info(index, count, unused)
        elif inst_type.instinfo is instructions.MultiANewArray:
            index, dimensions = self.read_u2(), self.read_u1()
            return inst_info(index, dimensions)
        elif inst_type.instinfo is instructions.NewArray:
            atype = instructions.ArrayType(self.read_u1())
            return inst_info(atype)
        elif inst_type.instinfo is instructions.LookupSwitch:
            self.read_align_bytes(current_method_offset + 1)
            default, npairs = self.read_i4(), self.read_u4()
            pairs = [
                instructions.MatchOffsetPair(self.read_i4(), self.read_u4())
                for _ in range(npairs)
            ]
            return inst_info(default, npairs, pairs)
        elif inst_type.instinfo is instructions.TableSwitch:
            self.read_align_bytes(current_method_offset + 1)
            default, low, high = self.read_i4(), self.read_i4(), self.read_i4()
            offsets = [self.read_i4() for _ in range(high - low + 1)]
            return inst_info(default, low, high, offsets)
        elif inst_type is instructions.InsnInfoType.WIDE:
            wide_opcode = self.read_u1()
            wide_inst_type = instructions.InsnInfoType(opcode + wide_opcode)
            wide_inst_info = partial(
                wide_inst_type.instinfo, wide_inst_type, current_method_offset
            )
            if wide_inst_type.instinfo is instructions.LocalIndexW:
                index = self.read_u2()
                return wide_inst_info(index)
            elif wide_inst_type.instinfo is instructions.IIncW:
                index, value = self.read_u2(), self.read_i2()
                return wide_inst_info(index, value)
        elif inst_type.instinfo is instructions.InsnInfo:
            return inst_info()

        raise Exception(f"Invalid InstInfoType: {inst_type.name} {inst_type.instinfo}")

    def read_code_bytes(self, code_length):
        start_method_offset = self.offset
        results = []
        while (
            current_method_offset := self.offset - start_method_offset
        ) < code_length:
            insn = self.read_instruction(current_method_offset)
            results.append(insn)
        return results

    def read_verification_type_info(self):
        tag = self.read_u1()
        match tag:
            case constants.VerificationType.TOP:
                return attributes.TopVariableInfo(tag)
            case constants.VerificationType.INTEGER:
                return attributes.IntegerVariableInfo(tag)
            case constants.VerificationType.FLOAT:
                return attributes.FloatVariableInfo(tag)
            case constants.VerificationType.DOUBLE:
                return attributes.DoubleVariableInfo(tag)
            case constants.VerificationType.LONG:
                return attributes.LongVariableInfo(tag)
            case constants.VerificationType.NULL:
                return attributes.NullVariableInfo(tag)
            case constants.VerificationType.UNINITIALIZED_THIS:
                return attributes.UninitializedThisVariableInfo(tag)
            case constants.VerificationType.OBJECT:
                return attributes.ObjectVariableInfo(tag, self.read_u2())
            case constants.VerificationType.UNINITIALIZED:
                return attributes.UninitializedVariableInfo(tag, self.read_u2())

    def read_element_value_info(self):
        tag = self.read_u1().to_bytes(1, "big").decode("ascii")

        match tag:
            case x if x in ("B", "C", "D" "F", "I", "J", "S", "Z", "s"):
                return attributes.ElementValueInfo(
                    tag, attributes.ConstValueInfo(self.read_u2())
                )
            case "e":
                return attributes.ElementValueInfo(
                    tag,
                    attributes.EnumConstantValueInfo(self.read_u2(), self.read_u2()),
                )
            case "c":
                return attributes.ElementValueInfo(
                    tag, attributes.ClassInfoValueInfo(self.read_u2())
                )
            case "@":
                return attributes.ElementValueInfo(tag, self.read_annotation_info())
            case "[":
                num_values = self.read_u2()
                values = [self.read_element_value_info() for _ in range(num_values)]
                return attributes.ElementValueInfo(
                    tag, attributes.ArrayValueInfo(num_values, values)
                )

    def read_annotation_info(self):
        type_index = self.read_u2()
        num_element_value_pairs = self.read_u2()
        element_value_pairs = [
            attributes.ElementValuePairInfo(
                self.read_u2(), self.read_element_value_info()
            )
            for _ in range(num_element_value_pairs)
        ]
        return attributes.AnnotationInfo(
            type_index, num_element_value_pairs, element_value_pairs
        )

    def read_target_info(self, target_type):
        match target_type:
            case x if x in constants.TargetInfoType.TYPE_PARAMETER:
                return attributes.TypeParameterTargetInfo(self.read_u1())
            case x if x in constants.TargetInfoType.SUPERTYPE:
                return attributes.SupertypeTargetInfo(self.read_u2())
            case x if x in constants.TargetInfoType.TYPE_PARAMETER_BOUND:
                return attributes.TypeParameterBoundTargetInfo(
                    self.read_u1(), self.read_u1()
                )
            case x if x in constants.TargetInfoType.EMPTY:
                return attributes.EmptyTargetInfo()
            case x if x in constants.TargetInfoType.FORMAL_PARAMETER:
                return attributes.FormalParameterTargetInfo(self.read_u1())
            case x if x in constants.TargetInfoType.THROWS:
                return attributes.ThrowsTargetInfo(self.read_u2())
            case x if x in constants.TargetInfoType.LOCALVAR:
                table_length = self.read_u2()
                table = [
                    attributes.TableInfo(self.read_u2(), self.read_u2(), self.read_u2())
                    for _ in range(table_length)
                ]
                return attributes.LocalvarTargetInfo(table_length, table)
            case x if x in constants.TargetInfoType.CATCH:
                return attributes.CatchTargetInfo(self.read_u2())
            case x if x in constants.TargetInfoType.OFFSET:
                return attributes.OffsetTargetInfo(self.read_u2())
            case x if x in constants.TargetInfoType.TYPE_ARGUMENT:
                return attributes.TypeArgumentTargetInfo(self.read_u2(), self.read_u1())

    def read_target_path(self):
        path_length = self.read_u1()
        path = [
            attributes.PathInfo(self.read_u1(), self.read_u1())
            for _ in range(path_length)
        ]
        return attributes.TypePathInfo(path_length, path)

    def read_type_annotation_info(self):
        target_type = self.read_u1()
        target_info = self.read_target_info(target_type)
        target_path = self.read_target_path()
        type_index = self.read_u2()
        num_element_value_pairs = self.read_u2()
        element_value_pairs = [
            attributes.ElementValuePairInfo(
                self.read_u2(), self.read_element_value_info()
            )
            for _ in range(num_element_value_pairs)
        ]
        return attributes.TypeAnnotationInfo(
            target_type,
            target_info,
            target_path,
            type_index,
            num_element_value_pairs,
            element_value_pairs,
        )

    def read_attribute(self):
        name_index, length = self.read_u2(), self.read_u4()

        name_cp = self.constant_pool[name_index]
        if not isinstance(name_cp, constant_pool.Utf8Info):
            raise ValueError(
                "name_index(%d) should be Utf8Info, not %s"
                % (name_index, type(name_cp))
            )

        name = name_cp.str_bytes.decode("utf8")
        attr_type = attributes.AttributeInfoType(name)
        attr_class = partial(attr_type.attr_class, name_index, length)

        if attr_type in (
            attributes.AttributeInfoType.SYNTHETIC,
            attributes.AttributeInfoType.DEPRECATED,
        ):
            return attr_class()

        elif attr_type in (
            attributes.AttributeInfoType.CONSTANT_VALUE,
            attributes.AttributeInfoType.SIGNATURE,
            attributes.AttributeInfoType.SOURCE_FILE,
            attributes.AttributeInfoType.MODULE_MAIN_CLASS,
            attributes.AttributeInfoType.NEST_HOST,
        ):
            return attr_class(self.read_u2())

        elif attr_type is attributes.AttributeInfoType.CODE:
            max_stack, max_locals = self.read_u2(), self.read_u2()
            code_length = self.read_u4()
            code = self.read_code_bytes(code_length)
            exception_table_length = self.read_u2()
            exception_table = [
                attributes.ExceptionInfo(
                    self.read_u2(), self.read_u2(), self.read_u2(), self.read_u2()
                )
                for _ in range(exception_table_length)
            ]
            attributes_count = self.read_u2()
            attributes_list = [self.read_attribute() for _ in range(attributes_count)]
            return attr_class(
                max_stack,
                max_locals,
                code_length,
                code,
                exception_table_length,
                exception_table,
                attributes_count,
                attributes_list,
            )

        elif attr_type in (attributes.AttributeInfoType.STACK_MAP_TABLE,):
            number_of_entries = self.read_u2()
            entries = []
            for _ in range(number_of_entries):
                frame_type = self.read_u1()

                match frame_type:
                    case x if x in range(0, 64):
                        entries.append(attributes.SameFrameInfo(frame_type))
                    case x if x in range(64, 128):
                        entries.append(
                            attributes.SameLocals1StackItemFrameInfo(
                                frame_type, self.read_verification_type_info()
                            )
                        )
                    case 247:
                        entries.append(
                            attributes.SameLocals1StackItemFrameExtendedInfo(
                                frame_type,
                                self.read_u2(),
                                self.read_verification_type_info(),
                            )
                        )
                    case x if x in range(248, 251):
                        entries.append(
                            attributes.ChopFrameInfo(frame_type, self.read_u2())
                        )
                    case 251:
                        entries.append(
                            attributes.SameFrameExtendedInfo(frame_type, self.read_u2())
                        )
                    case x if x in range(252, 255):
                        offset_delta = self.read_u2()
                        verification_type_infos = [
                            self.read_verification_type_info()
                            for __ in range(frame_type - 251)
                        ]
                        entries.append(
                            attributes.AppendFrameInfo(
                                frame_type, offset_delta, verification_type_infos
                            )
                        )
                    case 255:
                        offset_delta = self.read_u2()
                        number_of_locals = self.read_u2()
                        locals = [
                            self.read_verification_type_info()
                            for __ in range(number_of_locals)
                        ]
                        number_of_stack_items = self.read_u2()
                        stack = [
                            self.read_verification_type_info()
                            for __ in range(number_of_stack_items)
                        ]
                        entries.append(
                            attributes.FullFrameInfo(
                                frame_type,
                                offset_delta,
                                number_of_locals,
                                locals,
                                number_of_stack_items,
                                stack,
                            )
                        )

            return attr_class(number_of_entries, entries)

        elif attr_type is attributes.AttributeInfoType.EXCEPTIONS:
            number_of_exceptions = self.read_u2()
            exception_index_table = [
                self.read_u2() for _ in range(number_of_exceptions)
            ]
            return attr_class(number_of_exceptions, exception_index_table)

        elif attr_type is attributes.AttributeInfoType.INNER_CLASSES:
            number_of_classes = self.read_u2()
            classes = [
                attributes.InnerClassInfo(
                    self.read_u2(),
                    self.read_u2(),
                    self.read_u2(),
                    constants.NestedClassAccessFlag(self.read_u2()),
                )
                for _ in range(number_of_classes)
            ]
            return attr_class(number_of_classes, classes)

        elif attr_type is attributes.AttributeInfoType.ENCLOSING_METHOD:
            return attr_class(self.read_u2(), self.read_u2())

        elif attr_type is attributes.AttributeInfoType.SOURCE_DEBUG_EXTENSION:
            return attr_class(self.read_bytes(length).decode("utf-8"))

        elif attr_type is attributes.AttributeInfoType.LINE_NUMBER_TABLE:
            line_number_table_length = self.read_u2()
            line_number_table = [
                attributes.LineNumberInfo(self.read_u2(), self.read_u2())
                for _ in range(line_number_table_length)
            ]
            return attr_class(line_number_table_length, line_number_table)

        elif attr_type is attributes.AttributeInfoType.LOCAL_VARIABLE_TABLE:
            local_variable_table_length = self.read_u2()
            local_variable_table = [
                attributes.LocalVariableInfo(
                    self.read_u2(),
                    self.read_u2(),
                    self.read_u2(),
                    self.read_u2(),
                    self.read_u2(),
                )
                for _ in range(local_variable_table_length)
            ]
            return attr_class(local_variable_table_length, local_variable_table)

        elif attr_type is attributes.AttributeInfoType.LOCAL_VARIABLE_TYPE_TABLE:
            local_variable_type_table_length = self.read_u2()
            local_variable_type_table = [
                attributes.LocalVariableTypeInfo(
                    self.read_u2(),
                    self.read_u2(),
                    self.read_u2(),
                    self.read_u2(),
                    self.read_u2(),
                )
                for _ in range(local_variable_type_table_length)
            ]
            return attr_class(
                local_variable_type_table_length, local_variable_type_table
            )

        elif attr_type in (
            attributes.AttributeInfoType.RUNTIME_VISIBLE_ANNOTATIONS,
            attributes.AttributeInfoType.RUNTIME_INVISIBLE_ANNOTATIONS,
        ):
            num_annotations = self.read_u2()
            annotations = [self.read_annotation_info() for _ in range(num_annotations)]
            return attr_class(num_annotations, annotations)

        elif attr_type in (
            attributes.AttributeInfoType.RUNTIME_VISIBLE_PARAMETER_ANNOTATIONS,
            attributes.AttributeInfoType.RUNTIME_INVISIBLE_PARAMETER_ANNOTATIONS,
        ):
            num_parameters = self.read_u2()
            parameter_annotations = []
            for _ in range(num_parameters):
                num_annotations = self.read_u2()
                annotations = [
                    self.read_annotation_info() for _ in range(num_annotations)
                ]
                parameter_annotations.append(
                    attributes.ParameterAnnotationInfo(num_annotations, annotations)
                )
            return attr_class(num_annotations, annotations)

        elif attr_type in (
            attributes.AttributeInfoType.RUNTIME_VISIBLE_TYPE_ANNOTATIONS,
            attributes.AttributeInfoType.RUNTIME_INVISIBLE_TYPE_ANNOTATIONS,
        ):
            num_annotations = self.read_u2()
            annotations = [
                self.read_type_annotation_info() for _ in range(num_annotations)
            ]
            return attr_class(num_annotations, annotations)

        elif attr_type is attributes.AttributeInfoType.ANNOTATION_DEFAULT:
            return attr_class(
                self.read_u2(), self.read_u4(), self.read_element_value_info()
            )

        elif attr_type is attributes.AttributeInfoType.BOOTSTRAP_METHODS:
            num_bootstrap_methods = self.read_u2()
            bootstrap_methods = []
            for _ in range(num_bootstrap_methods):
                bootstrap_method_ref = self.read_u2()
                num_bootstrap_arguments = self.read_u2()
                bootstrap_arguments = [
                    self.read_u2() for __ in range(num_bootstrap_arguments)
                ]
                bootstrap_methods.append(
                    attributes.BootstrapMethodInfo(
                        bootstrap_method_ref,
                        num_bootstrap_arguments,
                        bootstrap_arguments,
                    )
                )
            return attr_class(num_bootstrap_methods, bootstrap_methods)

        elif attr_type is attributes.AttributeInfoType.METHOD_PARAMETERS:
            parameters_count = self.read_u1()
            parameters = [
                attributes.MethodParameterInfo(
                    self.read_u2(), constants.MethodParameterAccessFlag(self.read_u2())
                )
                for _ in range(parameters_count)
            ]
            return attr_class(parameters_count, parameters)

        elif attr_type is attributes.AttributeInfoType.MODULE:
            module_name_index = self.read_u2()
            module_flags = constants.ModuleAccessFlag(self.read_u2)
            module_version_index = self.read_u2()

            requires_count = self.read_u2()
            requires = [
                attributes.RequiresInfo(
                    self.read_u2(),
                    constants.ModuleRequiresAccessFlag(self.read_u2()),
                    self.read_u2(),
                )
                for _ in range(requires_count)
            ]

            exports_count = self.read_u2()
            exports = []
            for _ in range(exports_count):
                exports_index = self.read_u2()
                exports_flags = constants.ModuleExportsAccessFlag(self.read_u2())
                exports_to_count = self.read_u2()
                exports_to_index = [self.read_u2() for __ in range(exports_to_count)]
                exports.append(
                    attributes.ExportInfo(
                        exports_index, exports_flags, exports_to_count, exports_to_index
                    )
                )

            opens_count = self.read_u2()
            opens = []
            for _ in range(opens_count):
                opens_index = self.read_u2()
                opens_flags = constants.ModuleOpensAccessFlag(self.read_u2())
                opens_to_count = self.read_u2()
                opens_to_index = [self.read_u2() for __ in range(opens_to_count)]
                opens.append(
                    attributes.OpensInfo(
                        opens_index, opens_flags, opens_to_count, opens_to_index
                    )
                )

            uses_count = self.read_u2()
            uses = [self.read_u2() for _ in range(uses_count)]

            provides_count = self.read_u2()
            provides = []
            for _ in range(provides_count):
                provides_index = self.read_u2()
                provides_with_count = self.read_u2()
                provides_with_index = [
                    self.read_u2() for __ in range(provides_with_count)
                ]
                provides.appends(
                    attributes.ProvidesInfo(
                        provides_index, provides_with_count, provides
                    )
                )

            return attr_class(
                module_name_index,
                module_flags,
                module_version_index,
                requires_count,
                requires,
                exports_count,
                exports,
                opens_count,
                opens,
                uses_count,
                uses,
                provides_count,
                provides,
            )

        elif attr_type is attributes.AttributeInfoType.MODULE_PACKAGES:
            package_count = self.read_u2()
            package_index = [self.read_u2() for _ in range(package_count)]
            return attr_class(package_count, package_index)

        elif attr_type is attributes.AttributeInfoType.NEST_MEMBERS:
            number_of_classes = self.read_u2()
            classes = [self.read_u2() for _ in range(number_of_classes)]
            return attr_class(number_of_classes, classes)

        elif attr_type is attributes.AttributeInfoType.RECORD:
            components_count = self.read_u2()
            components = []
            for _ in range(components_count):
                name_index = self.read_u2()
                descriptor_index = self.read_u2()
                attributes_count = self.read_u2()
                _attributes = [self.read_attribute() for _ in range(attributes_count)]
                components.append(
                    attributes.RecordComponentInfo(
                        name_index, descriptor_index, attributes_count, _attributes
                    )
                )
            return attr_class(components_count, components)

        elif attr_type is attributes.AttributeInfoType.PERMITTED_SUBCLASSES:
            number_of_classes = self.read_u2()
            classes = [self.read_u2() for _ in range(number_of_classes)]
            return attr_class(number_of_classes, classes)

        return attr_class(self.read_bytes(length), attr_type)

    def read_field(self):
        access_flags = constants.FieldAccessFlag(self.read_u2())
        name_index = self.read_u2()
        descriptor_index = self.read_u2()
        attributes_count = self.read_u2()
        attributes = [self.read_attribute() for _ in range(attributes_count)]
        return info.FieldInfo(
            access_flags, name_index, descriptor_index, attributes_count, attributes
        )

    def read_method(self):
        access_flags = constants.MethodAccessFlag(self.read_u2())
        name_index = self.read_u2()
        descriptor_index = self.read_u2()
        attributes_count = self.read_u2()
        attributes = [self.read_attribute() for _ in range(attributes_count)]
        return info.MethodInfo(
            access_flags, name_index, descriptor_index, attributes_count, attributes
        )

    def read_class(self):
        self.rewind()
        magic = self.read_u4()
        if magic != constants.MAGIC:
            raise MalformedClassException(
                f"Invalid magic number 0x{magic:x}, requires 0x{constants.MAGIC:x}"
            )

        minor, major = self.read_u2(), self.read_u2()
        if major >= 56 and minor not in (0, 65535):
            raise MalformedClassException("Invalid version %d/%d" % (major, minor))

        cp_count = self.read_u2()

        self.constant_pool, index = [None] * cp_count, 1
        while index < cp_count:
            cp_info, index_extra = self.read_constant_pool_index(index)
            self.constant_pool[index] = cp_info
            index += 1 + index_extra

        access_flags = constants.ClassAccessFlag(self.read_u2())
        this_class = self.read_u2()
        super_class = self.read_u2()

        interfaces_count = self.read_u2()
        interfaces = [self.read_u2() for _ in range(interfaces_count)]

        fields_count = self.read_u2()
        fields = [self.read_field() for _ in range(fields_count)]

        methods_count = self.read_u2()
        methods = [self.read_method() for _ in range(methods_count)]

        attributes_count = self.read_u2()
        attributes = [self.read_attribute() for _ in range(attributes_count)]

        self.class_info = info.ClassFile(
            magic,
            minor,
            major,
            cp_count,
            self.constant_pool,
            access_flags,
            this_class,
            super_class,
            interfaces_count,
            interfaces,
            fields_count,
            fields,
            methods_count,
            methods,
            attributes_count,
            attributes,
        )
