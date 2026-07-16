from collections.abc import (
    Sequence,
    Mapping,
    Iterable,
    Iterator,
    Callable,
    Set,
    MutableSequence,
    MutableMapping,
    Sized,
    Container,
    Hashable,
)

# Registered builtin types match their collection ABCs.
print(isinstance([1, 2], Sequence), isinstance("abc", Sequence), isinstance((1,), Sequence))
print(isinstance({1, 2}, Sequence), isinstance({}, Sequence))
print(isinstance({}, Mapping), isinstance([], Mapping))
print(isinstance([], Iterable), isinstance(5, Iterable))
print(isinstance(iter([]), Iterator), isinstance([], Iterator))
print(isinstance(len, Callable), isinstance(5, Callable))
print(isinstance({1, 2}, Set), isinstance(frozenset(), Set), isinstance([], Set))
print(isinstance([], MutableSequence), isinstance((), MutableSequence))
print(isinstance({}, MutableMapping), isinstance({1: 2}, Mapping))
print(isinstance(range(5), Sequence), isinstance(b"x", Sequence))

# issubclass over registered builtins.
print(issubclass(list, Sequence), issubclass(dict, Mapping), issubclass(str, Sequence))
print(issubclass(tuple, Sequence), issubclass(set, Set), issubclass(list, MutableSequence))
print(issubclass(tuple, MutableSequence), issubclass(frozenset, Set))

# One-trick-pony ABCs are structural for user classes.
class MyIter:
    def __iter__(self):
        return iter([1, 2])


class MySized:
    def __len__(self):
        return 3


class Plain:
    pass


print(isinstance(MyIter(), Iterable), isinstance(Plain(), Iterable))
print(isinstance(MySized(), Sized), isinstance(Plain(), Sized))
# Collection ABCs do NOT match unregistered user classes.
print(isinstance(MyIter(), Sequence))

# Hashable.
print(isinstance(1, Hashable), isinstance((1, 2), Hashable), isinstance([1], Hashable))

# Generators are iterators; dict views are iterable.
print(isinstance((x for x in range(2)), Iterator))
print(isinstance({1: 2}.keys(), Iterable))

# Bad name raises ImportError.
try:
    from collections.abc import NotAThing
except ImportError:
    print("ImportError")

# The `import collections.abc as abc` form works too.
import collections.abc as abc

print(isinstance([1], abc.Sequence), issubclass(dict, abc.Mapping))
