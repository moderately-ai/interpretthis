print({k: v for k, v in zip("abc", [1, 2, 3])})
print({x: x**2 for x in range(5)})
print({v: k for k, v in {"a": 1, "b": 2}.items()})
print({k: v for k, v in [("x", 1), ("y", 2)] if v > 1})
matrix = {(i, j): i*j for i in range(3) for j in range(3)}
print(matrix[(2, 2)])
words = ["apple", "banana", "apple", "cherry"]
print({w: words.count(w) for w in set(words)} == {"apple": 2, "banana": 1, "cherry": 1})
print({str(i): i for i in range(3)})
nested = {k: {kk: vv for kk, vv in v.items()} for k, v in {"a": {"x": 1}}.items()}
print(nested)
print(len({x % 3: x for x in range(10)}))
