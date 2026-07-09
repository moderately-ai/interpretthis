# Basic @dataclass — annotated class attributes become constructor
# params; __init__, __repr__, __eq__ are synthesized. Pins CPython
# semantics: repr shape is `ClassName(field=value, ...)`, equality
# compares field-tuples, and __match_args__ is the field-name tuple.
from dataclasses import dataclass

@dataclass
class Point:
    x: int
    y: int

p = Point(3, 4)
print(p)
print(p.x)
print(p.y)
print(p == Point(3, 4))
print(p == Point(3, 5))

@dataclass
class Person:
    name: str
    age: int = 0
    nickname: str = ""

# Defaults honoured; positional + keyword mixing works.
print(Person("Alice"))
print(Person("Bob", 30))
print(Person("Carol", age=25, nickname="C"))

# __match_args__ is the tuple of field names (PEP 634 / B4 hook).
print(Person.__match_args__)
