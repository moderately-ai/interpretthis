# Construction from assorted iterables routes through the CPython-order table.
# (Float object identity — NaN dedup, `nan in {nan}` — is a documented residual
# in CONFORMANCE.md#set-print-order and deliberately not exercised here.)
print(set("mississippi"))
print(frozenset([3, 1, 2, 3, 1]))
d = {"x": 1, "y": 2, "z": 3}
print(set(d))
print(set(d.keys()) == set(d))
print(sorted(set(d.values())))
print(set(enumerate("ab")))

# frozenset as a set member and as a dict key: order-independent hashing means
# {1, 2} and {2, 1} collide to one element / one key.
s = {frozenset({1, 2}), frozenset({2, 1}), frozenset({3})}
print(len(s))
m = {frozenset({1, 2}): "a", frozenset({3}): "b"}
print(m[frozenset({2, 1})])

# set/frozenset repr with nested containers and mixed types.
print({(1, 2), (3, 4)})
print(frozenset({"a"}))
print({frozenset()})

# Decimal/Fraction now hash and compare across the numeric tower, so they key a
# table-backed set and are found there.
from decimal import Decimal
from fractions import Fraction

print(Decimal(3) in {3}, Fraction(1, 2) in {0.5})
print(len({Decimal("2"), 2, 2.0, Fraction(2, 1)}))
print(3.5 in {Fraction(7, 2)}, Decimal("0.5") in {Fraction(1, 2)})
