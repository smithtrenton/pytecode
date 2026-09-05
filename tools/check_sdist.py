"""Check source-distribution inputs before attempting an isolated wheel build."""

import sys
import tarfile
from pathlib import PurePosixPath


def main() -> None:
    with tarfile.open(sys.argv[1]) as archive:
        names = {PurePosixPath(*PurePosixPath(name).parts[1:]).as_posix() for name in archive.getnames()}
    required = {
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
        "pyproject.toml",
        "LICENSE",
        "pytecode/__init__.py",
        "pytecode/_rust.pyi",
        "pytecode/py.typed",
        "crates/pytecode-engine/src/writer/attributes.rs",
        "crates/pytecode-engine/src/model/stack_map.rs",
        "crates/pytecode-python/src/lib.rs",
        "crates/pytecode-python/src/model/constant_pool.rs",
        "crates/pytecode-archive/src/lib.rs",
        "crates/pytecode-cli/Cargo.toml",
        "crates/pytecode-cli/src/main.rs",
    }
    assert not (required - names), sorted(required - names)
    assert not any(name.startswith(("output/", ".venv/", ".git/", "target/")) for name in names)
    assert not any("/" not in name and name.endswith(".jar") for name in names)
    print(f"Source archive contains required build inputs ({len(names)} members).")


if __name__ == "__main__":
    main()
