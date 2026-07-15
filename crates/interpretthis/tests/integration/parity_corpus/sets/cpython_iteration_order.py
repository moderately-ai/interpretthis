# Sets iterate in CPython's hash-table order, not insertion order. This covers
# the constructions that build a set by insertion — set(iterable), set
# comprehensions, and .add() — plus every observation path (repr, iteration,
# unpacking, pop, frozenset-as-key). Constant set literals ({'a','b'}) go
# through CPython's compiler constant-fold and are handled separately.
print(set(range(20)))
print(set("mississippi"))
print(set(["apple", "banana", "cherry", "date"]))
print({i * i for i in range(10)})
print({c for c in "the quick brown fox"})
print({str(i) for i in range(15)})
print(list(set(["one", "two", "three", "four", "five", "six", "seven"])))
print(set(range(100, 130)))
print(frozenset(["x", "y", "z", "w", "v", "u", "t"]))

s = set()
for w in ["alpha", "beta", "gamma", "delta", "epsilon"]:
    s.add(w)
print(s)

# Observation paths.
a, b, c = set(["p", "q", "r"])
print(a, b, c)
print([*set(["m", "n", "o", "l"])])
print("".join(set("abcdef")))
first = set(["z1", "z2", "z3", "z4"]).pop()
print(first)
print({frozenset(set(["a", "b", "c"])): 1})

# Numbers hash to themselves; a non-trivial int set still reorders.
print(set(range(0, 40, 3)))
print(set([-5, 3, -8, 12, 0, 7]))
