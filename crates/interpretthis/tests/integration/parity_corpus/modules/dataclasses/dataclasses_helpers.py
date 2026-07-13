# `is_dataclass`, `asdict`, `astuple` helpers + nested dataclass repr.
#
# Pins CPython semantics:
#   - is_dataclass works on both the class and instances; non-dataclass
#     types return False.
#   - asdict returns a plain dict keyed by field name in declared order.
#   - astuple returns a plain tuple of field values in declared order.
#   - Nested dataclass containment renders the inner one in repr-shape
#     (`Outer(name='x', inner=Inner(value=1))`), not `<Inner object>`.
from dataclasses import dataclass, is_dataclass, asdict, astuple

@dataclass
class Inner:
    value: int

@dataclass
class Outer:
    name: str
    inner: Inner

@dataclass
class Empty:
    pass

print(is_dataclass(Inner))
print(is_dataclass(Outer))
print(is_dataclass(Empty))
print(is_dataclass(int))
print(is_dataclass("not a dataclass"))

i = Inner(value=42)
o = Outer(name="root", inner=i)

print(is_dataclass(i))
print(is_dataclass(o))

print(o)
print(asdict(o))
print(astuple(o))
