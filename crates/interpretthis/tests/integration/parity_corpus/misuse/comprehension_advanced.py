print([x*y for x in range(3) for y in range(3) if x != y])
print({x: [y for y in range(x)] for x in range(3)})
print([[i*j for j in range(3)] for i in range(3)])
print(sorted({x for x in "hello world" if x != " "}))
print(sum(x**2 for x in range(5)))
print(list(x + y for x, y in zip([1, 2], [10, 20])))
matrix = [[1, 2, 3], [4, 5, 6]]
print([sum(row) for row in matrix])
print([col for row in matrix for col in row])
print({k: v*2 for k, v in {"a": 1, "b": 2}.items()})
data = [("a", 1), ("b", 2), ("a", 3)]
grouped = {}
[grouped.setdefault(k, []).append(v) for k, v in data]
print(grouped)
print([x for x in range(20) if x % 2 == 0 if x % 3 == 0])
print(any(x > 5 for x in range(10)))
print(all(x >= 0 for x in range(10)))
print(max((x, -x) for x in range(5)))
words = ["hello", "world", "python"]
print({w: len(w) for w in words})
