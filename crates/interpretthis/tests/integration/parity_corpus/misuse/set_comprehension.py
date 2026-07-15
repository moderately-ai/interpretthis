print(sorted({x % 3 for x in range(10)}))
print(sorted({x * y for x in range(3) for y in range(3)}))
print(len({x for x in "mississippi"}))
print(sorted({len(w) for w in ["a", "bb", "ccc", "dd"]}))
matrix = [[1, 2, 3], [4, 5, 6]]
print(sorted({n for row in matrix for n in row}))
print({x for x in range(5) if x % 2 == 0} == {0, 2, 4})
print(sorted({abs(x) for x in [-1, 1, -2, 2, -3]}))
words = ["apple", "banana", "cherry"]
print(sorted({w[0] for w in words}))
