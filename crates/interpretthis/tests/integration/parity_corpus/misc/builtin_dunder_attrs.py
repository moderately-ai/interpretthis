# Builtin objects expose their dunder methods to hasattr/getattr, matching
# CPython's per-type attribute sets (a common duck-typing idiom). Presence is
# precise per type: dict has no __add__, set has __or__ but no __add__, int has
# __or__ but str/list do not, etc.
print(hasattr([], "__iter__"), hasattr([], "__len__"), hasattr([], "__getitem__"))
print(hasattr({}, "__iter__"), hasattr({}, "__contains__"), hasattr({}, "__add__"))
print(hasattr("s", "__iter__"), hasattr("s", "__len__"), hasattr("s", "__mul__"))
print(hasattr(5, "__iter__"), hasattr(5, "__add__"), hasattr(5, "__int__"))
print(hasattr(5, "__or__"), hasattr("s", "__or__"), hasattr([], "__or__"))
print(hasattr(set(), "__or__"), hasattr(set(), "__add__"), hasattr(frozenset(), "__and__"))
print(hasattr((1,), "__iter__"), hasattr(range(3), "__iter__"), hasattr(range(3), "__reversed__"))
print(hasattr(iter([]), "__next__"), hasattr(iter([]), "__iter__"))
print(hasattr((x for x in []), "__next__"), hasattr((x for x in []), "__iter__"))
print(hasattr(5, "__eq__"), hasattr(5.0, "__float__"), hasattr(3.14, "__add__"))
print(hasattr(True, "__bool__"), hasattr(None, "__bool__"), hasattr(5, "__contains__"))
print(hasattr(b"x", "__iter__"), hasattr(bytearray(), "__setitem__"), hasattr((), "__setitem__"))
print(hasattr([], "__hash__"), hasattr(5, "__hash__"), hasattr([], "__dict__"))
print(hasattr(5, "__str__"), hasattr([], "__repr__"), hasattr("x", "__eq__"))
print(callable([].__iter__), callable((5).__add__), callable("x".__len__))

# The resolved method-wrappers are callable and route to the operation.
print((5).__add__(3), (10).__sub__(3), (5).__mul__(4), (7).__mod__(3), (2).__pow__(10))
print("abc".__len__(), [1, 2, 3].__len__(), {1, 2}.__len__())
print([1, 2, 3].__getitem__(1), "hello".__getitem__(0))
print((5).__eq__(5), (5).__lt__(10), (5).__ne__(6))
print("a".__add__("b"), [1].__add__([2]))
print((-5).__abs__(), (3.14).__int__(), (5).__hash__())
print([1, 2, 3].__contains__(2), "cat".__contains__("a"))
print((5).__str__(), [1, 2].__repr__())
print(list([1, 2, 3].__iter__()), list("abc".__iter__()), sorted({3, 1, 2}.__iter__()))
xs = [10, 20, 30]
it = xs.__iter__()
print(next(it), next(it), next(it))
print(sorted({"a": 1, "b": 2}.__iter__()), list(range(3).__iter__()))
length = [1, 2, 3, 4].__len__
print(length())
