import sys

import IPython
import pytecode

cr = pytecode.ClassReader.from_file(sys.argv[1])
print(cr.file_name, "=", cr.class_info)

print(cr.class_info.class_name)
print(cr.class_info.access_flags)
print(cr.class_info.super_name)
print(cr.class_info.interfaces)
print(cr.class_info.fields)
print(cr.class_info.methods)

IPython.embed()