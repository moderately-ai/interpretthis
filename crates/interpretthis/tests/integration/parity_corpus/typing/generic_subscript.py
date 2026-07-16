from typing import TypeVar, Generic

T = TypeVar("T")


class Stack(Generic[T]):
    def __init__(self):
        self.items = []

    def push(self, x):
        self.items.append(x)

    def pop(self):
        return self.items.pop()


# Subscripting a Generic user class type-erases: Stack[int]() instantiates Stack.
s = Stack[int]()
s.push(1)
s.push(2)
print(s.pop(), s.items)


class Box(Generic[T]):
    def __init__(self, v):
        self.v = v


print(Box[str]("hello").v)


# PEP 585 builtin generic aliases repr bare and are only defined on containers.
print(list[int], dict[str, int], tuple[int, str])
print(set[int], frozenset[str], type[int])
print(list[dict[str, int]])
print(str(list[int]), repr(dict[str, int]))
for bad, fn in (("int", lambda: int[int]), ("str", lambda: str[int]), ("float", lambda: float[int])):
    try:
        fn()
    except TypeError:
        print(bad, "not subscriptable")


# A class defining __class_getitem__ has it called on subscript.
class WithCGI:
    def __class_getitem__(cls, item):
        return f"{cls.__name__}[got]"


print(WithCGI[int])
