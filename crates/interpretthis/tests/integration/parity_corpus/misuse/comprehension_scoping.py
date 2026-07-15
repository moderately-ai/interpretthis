x = 10
result = [x for x in range(5)]
print(result)
print(x)
y = 100
gen_result = list(y for y in range(3))
print(gen_result)
print(y)
data = [1, 2, 3]
squared = {n: n**2 for n in data}
print(squared)
nested = [[y for y in range(x)] for x in range(4)]
print(nested)
total = sum(i*j for i in range(3) for j in range(3))
print(total)
filtered = [w.upper() for w in ["a", "b", "c"] if w != "b"]
print(filtered)
matrix = [[1, 2], [3, 4]]
flat = [elem for row in matrix for elem in row]
print(flat)
