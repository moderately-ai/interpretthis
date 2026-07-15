# Comprehension scoping, nesting, conditionals, walrus.
matrix = [[1, 2, 3], [4, 5, 6], [7, 8, 9]]
print([x for row in matrix for x in row])
print([[row[i] for row in matrix] for i in range(3)])
print({x: x**2 for x in range(5)})
print({x % 3 for x in range(10)})
print([x for x in range(20) if x % 2 == 0 if x % 3 == 0])
print([x if x > 0 else -x for x in [-2, 3, -4, 5]])

# nested with dependency
print([(i, j) for i in range(3) for j in range(i)])

# walrus in comprehension
print([y for x in range(5) if (y := x * 2) > 4])

# comprehension doesn't leak loop var
z = 100
print([z for z in range(3)], z)

# dict/set comprehension with condition
print({k: v for k, v in [("a", 1), ("b", 2), ("c", 3)] if v > 1})

# generator in sum/any/all
print(sum(x for x in range(10)), any(x > 5 for x in range(3)), all(x < 10 for x in range(5)))

# nested dict comprehension
print({i: {j: i * j for j in range(3)} for i in range(3)})
