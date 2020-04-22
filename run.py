import sys

import pytecode

cr = pytecode.ClassReader.from_file(sys.argv[1])
print(cr.file_name, "=", cr.class_info)
