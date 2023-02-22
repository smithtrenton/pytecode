import sys
from pprint import pprint

import IPython
import pytecode

cr = pytecode.ClassReader.from_file(sys.argv[1])

pprint(cr.file_name)
pprint(cr.class_info)

IPython.embed()
