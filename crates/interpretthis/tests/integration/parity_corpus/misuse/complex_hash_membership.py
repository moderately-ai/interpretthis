# complex is hashable and equal to a real int/float with zero imaginary part,
# so it unifies with the numeric tower in sets and dict keys.
print(len({1 + 0j, 1, 1.0, True}))
print((1 + 0j) in {1})
print(1 in {1 + 0j})
print({1 + 0j: "a"}[1])
print({1: "a"}[1 + 0j])
print(len({1 + 2j, complex(1, 2), 3 + 4j}))
print((1 + 2j) in {complex(1, 2)})

# A non-real complex is not equal to any real, so it stays distinct.
print(len({1 + 2j, 1}))
print((1 + 2j) in {1})

# hash equality drives set/dict collapse: hash(complex(x, 0)) == hash(x).
print(hash(1 + 0j) == hash(1), hash(2.0 + 0j) == hash(2.0))
print({complex(0, 0), 0, 0.0, False})
