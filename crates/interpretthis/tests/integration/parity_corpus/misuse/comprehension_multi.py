print([x*y for x in range(3) for y in range(3)])
print([x for x in range(10) if x % 2 == 0 if x > 2])
print({x: x**2 for x in range(4)})
print({x % 3 for x in range(10)})
matrix = [[1, 2], [3, 4]]
print([n for row in matrix for n in row])
print([[row[i] for row in matrix] for i in range(2)])
print(list(x for x in range(3)))
print(sum(x*2 for x in range(5)))
