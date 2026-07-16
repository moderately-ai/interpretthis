# @enum.unique validates that no two members share a value (aliases), raising
# ValueError with CPython's exact wording; a clean enum passes through unchanged.
from enum import Enum, unique


@unique
class Good(Enum):
    A = 1
    B = 2
    C = 3


print(list(Good), [g.value for g in Good])


@unique
class Colors(Enum):
    RED = "red"
    GREEN = "green"


print([c.value for c in Colors])


def trap(build):
    try:
        build()
        return "no-trap"
    except ValueError as e:
        return str(e)


def bad1():
    @unique
    class Bad(Enum):
        X = 1
        Y = 1
        Z = 2


def bad2():
    @unique
    class Bad2(Enum):
        P = 1
        Q = 2
        R = 1
        S = 2


print(trap(bad1))
print(trap(bad2))
