print(list(enumerate(["a", "b", "c"])))
print(list(enumerate("xyz", start=10)))
print(list(zip([1,2,3], [4,5,6])))
print(list(zip("ab", "cd", "ef")))
print(list(zip([1,2,3], [4,5])))
print(dict(enumerate("abc")))
print([f"{i}: {v}" for i, v in enumerate(["x", "y"])])
print(list(zip(*[[1,2,3], [4,5,6]])))
for i, (a, b) in enumerate(zip([1,2], [3,4])):
    print(i, a, b)
print(list(map(lambda x, y: x + y, [1,2,3], [10,20,30])))
names = ["Alice", "Bob"]
ages = [30, 25]
print(dict(zip(names, ages)))
print(sum(i * v for i, v in enumerate([10, 20, 30])))
print(list(reversed(list(enumerate("ab")))))
print([x for pair in zip([1,2], [3,4]) for x in pair])
matrix = [[1,2],[3,4]]
print(list(zip(*matrix)))
