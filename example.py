import pytecode

cr = pytecode.ClassReader.from_file('HelloWorld.class')
print(cr.file_name, '=', cr.class_info)
