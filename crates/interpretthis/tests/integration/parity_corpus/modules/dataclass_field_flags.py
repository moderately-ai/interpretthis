# dataclasses.field init/repr/compare use truthiness, not a strict bool.
# Regression: a non-bool flag (field(init=0)) fell through as_bool and defaulted
# to True, so init=0 did not exclude the field from __init__.
from dataclasses import dataclass, field


@dataclass
class C:
    a: int
    b: int = field(default=5, init=0)      # falsy -> init=False (not an init arg)
    c: int = field(default=3, repr=0)       # falsy -> repr=False (hidden in repr)


# __init__ is (self, a, c): b is excluded, so a second positional binds c, not b.
print(C(1))                 # C(a=1, b=5)  — c hidden by repr=0
print(C(1, 2).b, C(1, 2).c) # 5 2  (with the bug: 2 3)
try:
    C(1, 2, 3)              # only a and c are init params -> too many positionals
except TypeError:
    print("TypeError")


@dataclass
class D:
    x: int = field(default=1, init=True)    # a real bool still works


print(D())
