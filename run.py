import os
import sys
import time
from pprint import pprint

import pytecode


input_jar_fn = os.path.abspath(sys.argv[1])
jar_dir = os.path.dirname(input_jar_fn)
jar_fn = os.path.basename(input_jar_fn)
jar_name = jar_fn.split('.jar')[0]
output_dir = os.path.join(jar_dir, 'output', jar_name)

start = time.time()

jar = pytecode.JarFile(sys.argv[1])

end = time.time()
print(f'Read time: {end - start}s')

start = time.time()

classes, other_files = jar.parse_classes()

end = time.time()
print(f'Parse time: {end - start}s')
print(f'\tclasses: {len(classes)}')
print(f'\tother_files: {len(other_files)}')

start = time.time()

for c in classes:
    fn = os.path.join(output_dir, c[0].filename) + '.output'
    os.makedirs(os.path.dirname(fn), exist_ok=True)
    with open(fn, 'w') as f:
        pprint(c[1].class_info, f)

for of in other_files:
    fn = os.path.join(output_dir, of.filename)
    os.makedirs(os.path.dirname(fn), exist_ok=True)
    with open(fn, 'wb') as f:
        f.write(of.bytes)

end = time.time()
print(f'Write time: {end - start}s')
print(f'\tdir: {output_dir}')
