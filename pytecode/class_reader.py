from functools import partial
from struct import unpack_from

from .types import attributes, constants, constant_pool, info


def _read_u4(_bytes, offset=0):
    return unpack_from('>I', _bytes, offset)[0]


def _read_u2(_bytes, offset=0):
    return unpack_from('>H', _bytes, offset)[0]


def _read_u1(_bytes, offset=0):
    return unpack_from('>B', _bytes, offset)[0]


def _read_bytes(_bytes, length, offset=0):
    return unpack_from('>%ds' % length, _bytes, offset)[0]


class MalformedClassException(Exception):
    pass


class ClassReader:
    def __init__(self, file_name, _bytes):
        self.file_name = file_name
        self.bytes = _bytes
        self.offset = 0
        self.constant_pool = None
        self._read_class()

    @classmethod
    def from_file(cls, path):
        with open(path, 'rb') as f:
            file_bytes = f.read()
        return cls(path, file_bytes)

    def _read_u4(self):
        res = _read_u4(self.bytes, self.offset)
        self.offset += 4
        return res

    def _read_u2(self):
        res = _read_u2(self.bytes, self.offset)
        self.offset += 2
        return res

    def _read_u1(self):
        res = _read_u1(self.bytes, self.offset)
        self.offset += 1
        return res

    def _read_bytes(self, length):
        res = _read_bytes(self.bytes, length, self.offset)
        self.offset += length
        return res

    def _read_constant_pool_index(self, index):
        index_extra, offset, tag = 0, self.offset, self._read_u1()
        cp_type = constant_pool.ConstantPoolInfoType(tag)
        cp_class = partial(cp_type.cp_class, index, offset, tag)

        if cp_type in (constant_pool.ConstantPoolInfoType.CLASS,
                       constant_pool.ConstantPoolInfoType.STRING,
                       constant_pool.ConstantPoolInfoType.METHOD_TYPE,
                       constant_pool.ConstantPoolInfoType.MODULE,
                       constant_pool.ConstantPoolInfoType.PACKAGE):
            cp_info = cp_class(self._read_u2())
        elif cp_type in (constant_pool.ConstantPoolInfoType.FIELD_REF,
                         constant_pool.ConstantPoolInfoType.METHOD_REF,
                         constant_pool.ConstantPoolInfoType.INTERFACE_METHOD_REF,
                         constant_pool.ConstantPoolInfoType.NAME_AND_TYPE,
                         constant_pool.ConstantPoolInfoType.DYNAMIC,
                         constant_pool.ConstantPoolInfoType.INVOKE_DYNAMIC):
            cp_info = cp_class(self._read_u2(), self._read_u2())
        elif cp_type in (constant_pool.ConstantPoolInfoType.INTEGER,
                         constant_pool.ConstantPoolInfoType.FLOAT):
            cp_info = cp_class(self._read_u4())
        elif cp_type in (constant_pool.ConstantPoolInfoType.LONG,
                         constant_pool.ConstantPoolInfoType.DOUBLE):
            cp_info = cp_class(self._read_u4(), self._read_u4())
            index_extra = 1
        elif cp_type is constant_pool.ConstantPoolInfoType.UTF8:
            length = self._read_u2()
            str_bytes = self._read_bytes(length)
            cp_info = cp_class(length, str_bytes)
        elif cp_type is constant_pool.ConstantPoolInfoType.METHOD_HANDLE:
            cp_info = cp_class(self._read_u1(), self._read_u2())
        else:
            raise ValueError('Unknown ConstantPoolInfoType: %s' % cp_type)
        return cp_info, index_extra

    def _read_attribute(self):
        name_index, length = self._read_u2(), self._read_u4()

        name_cp = self.constant_pool[name_index]
        if not isinstance(name_cp, constant_pool.Utf8Info):
            raise ValueError('name_index(%d) should be Utf8Info, not %s' % (name_index, type(name_cp)))

        name = name_cp.str_bytes.decode('utf8')
        attr_type = attributes.AttributeInfoType(name)
        attr_class = partial(attr_type.attr_class, name_index, length)

        if attr_type in (attributes.AttributeInfoType.CONSTANT_VALUE,
                         attributes.AttributeInfoType.SIGNATURE,
                         attributes.AttributeInfoType.SOURCE_FILE,
                         attributes.AttributeInfoType.MODULE_MAIN_CLASS,
                         attributes.AttributeInfoType.NEST_HOST):
            return attr_class(self._read_u2())

        elif attr_type is attributes.AttributeInfoType.CODE:
            max_stack, max_locals = self._read_u2(), self._read_u2()
            code_length = self._read_u4()
            code = self._read_bytes(code_length)
            exception_table_length = self._read_u2()
            exception_table = []
            for _ in range(exception_table_length):
                exception_table.append(attributes.ExceptionInfo(self._read_u2(), self._read_u2(), self._read_u2(), self._read_u2()))
            attributes_count = self._read_u2()
            attributes_list = []
            for _ in range(attributes_count):
                attributes_list.append(self._read_attribute())
            return attr_class(max_stack, max_locals, code_length, code, exception_table_length, exception_table, attributes_count, attributes_list)

        elif attr_type is attributes.AttributeInfoType.EXCEPTIONS:
            number_of_exceptions = self._read_u2()
            exception_index_table = []
            for _ in range(number_of_exceptions):
                exception_index_table.append(self._read_u2())
            return attr_class(number_of_exceptions, exception_index_table)

        elif attr_type is attributes.AttributeInfoType.INNER_CLASSES:
            number_of_classes = self._read_u2()
            classes = []
            for _ in range(number_of_classes):
                classes.append(attributes.InnerClassInfo(self._read_u2(), self._read_u2(), self._read_u2(), constants.NestedClassAccessFlag(self._read_u2())))
            return attr_class(number_of_classes, classes)

        elif attr_type is attributes.AttributeInfoType.ENCLOSING_METHOD:
            return attr_class(self._read_u2(), self._read_u2())

        elif attr_type in (attributes.AttributeInfoType.SYNTHETIC,
                           attributes.AttributeInfoType.DEPRECATED):
            return attr_class()

        elif attr_type is attributes.AttributeInfoType.SOURCE_DEBUG_EXTENSION:
            return attr_class(self._read_bytes(length))

        elif attr_type is attributes.AttributeInfoType.LINE_NUMBER_TABLE:
            line_number_table_length = self._read_u2()
            line_number_table = []
            for _ in range(line_number_table_length):
                line_number_table.append(attributes.LineNumberInfo(self._read_u2(), self._read_u2()))
            return attr_class(line_number_table_length, line_number_table)

        elif attr_type is attributes.AttributeInfoType.LOCAL_VARIABLE_TABLE:
            local_variable_table_length = self._read_u2()
            local_variable_table = []
            for _ in range(local_variable_table_length):
                local_variable_table.append(attributes.LocalVariableInfo(self._read_u2(), self._read_u2(), self._read_u2(), self._read_u2(), self._read_u2()))
            return attr_class(local_variable_table_length, local_variable_table)

        elif attr_type is attributes.AttributeInfoType.LOCAL_VARIABLE_TYPE_TABLE:
            local_variable_type_table_length = self._read_u2()
            local_variable_type_table = []
            for _ in range(local_variable_type_table_length):
                local_variable_type_table.append(attributes.LocalVariableTypeInfo(self._read_u2(), self._read_u2(), self._read_u2(), self._read_u2(), self._read_u2()))
            return attr_class(local_variable_type_table_length, local_variable_type_table)

        elif attr_type is attributes.AttributeInfoType.BOOTSTRAP_METHODS:
            num_bootstrap_methods = self._read_u2()
            bootstrap_methods = []
            for _ in range(num_bootstrap_methods):
                bootstrap_method_ref = self._read_u2()
                num_bootstrap_arguments = self._read_u2()
                bootstrap_arguments = []
                for __ in range(num_bootstrap_arguments):
                    bootstrap_arguments.append(self._read_u2())
                bootstrap_methods.append(attributes.BootstrapMethod(bootstrap_method_ref, num_bootstrap_arguments, bootstrap_arguments))
            return attr_class(num_bootstrap_methods, bootstrap_methods)

        elif attr_type is attributes.AttributeInfoType.METHOD_PARAMETERS:
            parameters_count = self._read_u1()
            parameters = []
            for _ in range(parameters_count):
                parameters.append(attributes.MethodParameter(self._read_u2(), constants.MethodParameterAccessFlag(self._read_u2())))
            return attr_class(parameters_count, parameters)

        elif attr_type is attributes.AttributeInfoType.MODULE:
            module_name_index = self._read_u2()
            module_flags = constants.ModuleAccessFlag(self._read_u2)
            module_version_index = self.read_u2()

            requires_count = self._read_u2()
            requires = []
            for _ in range(requires_count):
                requires.append(attributes.RequiresInfo(self._read_u2(), constants.ModuleRequiresAccessFlag(self._read_u2()), self._read_u2()))

            exports_count = self._read_u2()
            exports = []
            for _ in range(exports_count):
                exports_index = self._read_u2()
                exports_flags = constants.ModuleExportsAccessFlag(self._read_u2())
                exports_to_count = self._read_u2()
                exports_to_index = []
                for __ in range(exports_to_count):
                    exports_to_index.append(self._read_u2())
                exports.append(attributes.ExportsInfo(exports_index, exports_flags, exports_to_count, exports_to_index))

            opens_count = self._read_u2()
            opens = []
            for _ in range(opens_count):
                opens_index = self._read_u2()
                opens_flags = constants.ModuleOpensAccessFlag(self._read_u2())
                opens_to_count = self._read_u2()
                opens_to_index = []
                for __ in range(opens_to_count):
                    opens_to_index.append(self._read_u2())
                opens.append(attributes.OpensInfo(opens_index, opens_flags, opens_to_count, opens_to_index))

            uses_count = self._read_u2()
            uses = []
            for _ in range(uses_count):
                uses.append(self._read_u2())

            provides_count = self._read_u2()
            provides = []
            for _ in range(provides_count):
                provides_index = self._read_u2()
                provides_with_count = self._read_u2()
                provides_with_index = []
                for __ in range(provides_with_count):
                    provides_with_index.append(self._read_u2())
                provides.appends(attributes.ProvidesInfo(provides_index, provides_with_count, provides))

            return attr_class(module_name_index, module_flags, module_version_index, requires_count, requires,
                              exports_count, exports, opens_count, opens, uses_count, uses, provides_count, provides)

        elif attr_type is attributes.AttributeInfoType.MODULE_PACKAGES:
            package_count = self._read_u2()
            package_index = []
            for _ in range(package_count):
                package_index.append(self._read_u2())
            return attr_class(package_count, package_index)

        elif attr_type is attributes.AttributeInfoType.NEST_MEMBERS:
            number_of_classes = self._read_u2()
            classes = []
            for _ in range(number_of_classes):
                classes.append(self._read_u2())
            return attr_class(number_of_classes, classes)

        return attributes.UnimplementedAttr(name_index, length, self._read_bytes(length), attr_type)

    def _read_field(self):
        access_flags = constants.FieldAccessFlag(self._read_u2())
        name_index = self._read_u2()
        descriptor_index = self._read_u2()
        attributes_count = self._read_u2()
        attributes = []
        for i in range(attributes_count):
            attributes.append(self._read_attribute())
        return info.FieldInfo(access_flags, name_index, descriptor_index, attributes_count, attributes)

    def _read_method(self):
        access_flags = constants.MethodAccessFlag(self._read_u2())
        name_index = self._read_u2()
        descriptor_index = self._read_u2()
        attributes_count = self._read_u2()
        attributes = []
        for i in range(attributes_count):
            attributes.append(self._read_attribute())
        return info.MethodInfo(access_flags, name_index, descriptor_index, attributes_count, attributes)

    def _read_class(self):
        self.offset = 0
        magic = self._read_u4()
        if magic != constants.MAGIC:
            raise MalformedClassException('Invalid magic number %d' % magic)

        minor, major = self._read_u2(), self._read_u2()
        if major >= 56 and minor not in (0, 65535):
            raise MalformedClassException('Invalid version %d/%d' % (major, minor))

        cp_count = self._read_u2()

        self.constant_pool, index = [None] * cp_count, 1
        while index < cp_count:
            cp_info, index_extra = self._read_constant_pool_index(index)
            self.constant_pool[index] = cp_info
            index += (1 + index_extra)

        access_flags = constants.ClassAccessFlag(self._read_u2())
        this_class = self._read_u2()
        super_class = self._read_u2()

        interfaces_count = self._read_u2()
        interfaces = []
        for i in range(interfaces_count):
            interfaces.append(self._read_u2())

        fields_count = self._read_u2()
        fields = []
        for i in range(fields_count):
            fields.append(self._read_field())

        methods_count = self._read_u2()
        methods = []
        for i in range(methods_count):
            methods.append(self._read_method())

        attributes_count = self._read_u2()
        attributes = []
        for i in range(attributes_count):
            attributes.append(self._read_attribute())

        self.class_info = info.ClassInfo(magic, minor, major, cp_count, self.constant_pool, access_flags, this_class, super_class,
                              interfaces_count, interfaces, fields_count, fields, methods_count, methods,
                              attributes_count, attributes)