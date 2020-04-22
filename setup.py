from setuptools import setup

with open("README") as f:
    long_desc = f.read()

setup(
    name="pytecode",
    version="0.0.1",
    url="github.com/smithtrenton/pytecode",
    license="LICENSE",
    description="Python library for parsing and analyzing JVM bytecode",
    long_description=long_desc,
    packages=["pytecode"],
)
