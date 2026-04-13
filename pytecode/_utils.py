"""Internal shared utilities — not part of the public API."""

from __future__ import annotations

from collections.abc import Callable


def document_property(
    cls: type[object],
    name: str,
    doc: str,
    return_annotation: object,
    *,
    writable: bool = False,
) -> None:
    """Attach a documented ``property`` wrapper around a Rust-backed descriptor."""

    descriptor = cls.__dict__[name]

    def getter(self: object) -> object:
        return descriptor.__get__(self, type(self))

    getter.__name__ = name
    getter.__doc__ = doc
    getter.__annotations__ = {"return": return_annotation}

    setter_func: Callable[[object, object], None] | None = None
    if writable:

        def _setter(self: object, value: object) -> None:
            descriptor.__set__(self, value)

        _setter.__name__ = name
        _setter.__annotations__ = {"value": return_annotation, "return": None}
        setter_func = _setter

    setattr(cls, name, property(getter, setter_func, doc=doc))
