# frozenset is an immutable, hashable set: it constructs from any iterable,
# works as a dict key and set member, supports the non-mutating set algebra,
# and rejects mutation. Regression: the interpreter had no frozenset at all.
fs = frozenset([3, 1, 2, 2, 1])
print(len(fs))
print(type(fs).__name__)
print(2 in fs, 5 in fs)
print(sorted(fs))

# Equality is order-independent and cross-type with set.
print(frozenset([1, 2]) == frozenset([2, 1]))
print(frozenset([1, 2]) == {2, 1})
print(frozenset([1, 2]) == frozenset([1, 2, 3]))

# Hashable: usable as a dict key and inside a set.
d = {frozenset([1, 2]): "a", frozenset([3]): "b"}
print(d[frozenset([2, 1])])
s = {frozenset([1]), frozenset([1]), frozenset([2])}
print(len(s))

# Set algebra returns frozensets.
a = frozenset([1, 2, 3])
b = frozenset([2, 3, 4])
print(sorted(a | b))
print(sorted(a & b))
print(sorted(a - b))
print(sorted(a ^ b))
print(type(a | b).__name__)
print(a.issubset(frozenset([1, 2, 3, 4])), a.isdisjoint(frozenset([9])))
print(sorted(a.union([5, 6])))

# Empty frozenset repr.
print(frozenset())
print(repr(frozenset([1])))

# Immutable: no mutating methods.
try:
    fs.add(9)
except AttributeError as e:
    print("no add:", type(e).__name__)

# Mixed operand types: result takes the left operand's type.
print(type(frozenset([1]) | {2}).__name__)
print(type({1} | frozenset([2])).__name__)
