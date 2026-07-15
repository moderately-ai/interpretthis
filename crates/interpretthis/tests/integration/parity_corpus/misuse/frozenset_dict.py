d = {frozenset([1, 2]): "a", frozenset([3, 4]): "b"}
print(d[frozenset([2, 1])])
s = {frozenset([1]), frozenset([2]), frozenset([1])}
print(len(s))
cache = {}
cache[frozenset({"x", "y"})] = 42
print(cache[frozenset({"y", "x"})])
print(frozenset([1, 2, 3]) <= frozenset([1, 2, 3, 4]))
print(frozenset([1, 2]) | frozenset([2, 3]) == frozenset([1, 2, 3]))
grouped = {}
for word in ["cat", "dog", "car", "dot"]:
    key = frozenset(word)
    grouped.setdefault(key, []).append(word)
print(len(grouped))
matrix_keys = {(0, 0): "a", (1, 1): "b"}
print(matrix_keys[(0, 0)])
print((1, 2, 3) in {(1, 2, 3): "found"})
