from setuptools import setup

with open("README.md", "r", encoding="utf-8") as f:
    long_desc = f.read()

setup(
    name="pytecode",
    version="0.0.1",
    url="github.com/smithtrenton/pytecode",
    license="LICENSE",
    description="Python library for parsing and analyzing JVM bytecode",
    long_description=long_desc,
    packages=["pytecode"],
    python_requires=">=3.14",
    long_description_content_type="text/markdown",
    classifiers=[
        "Programming Language :: Python :: 3",
        "Programming Language :: Python :: 3.14",
    ],
)
