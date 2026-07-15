data = {"users": [{"name": "Alice", "age": 30}, {"name": "Bob", "age": 25}]}
print(data["users"][0]["name"])
print([u["age"] for u in data["users"]])
matrix = [[1, 2, 3], [4, 5, 6], [7, 8, 9]]
print([[matrix[j][i] for j in range(3)] for i in range(3)])
nested = {"a": {"b": {"c": [1, 2, {"d": "deep"}]}}}
print(nested["a"]["b"]["c"][2]["d"])
print({"x": [1, 2]} == {"x": [1, 2]})
print([{"a": 1}] == [{"a": 1}])
grid = [[0] * 3 for _ in range(3)]
grid[1][1] = 5
print(grid)
combined = {**{"a": 1}, **{"b": [1, 2, 3]}}
print(combined)
print(sorted([{"n": 3}, {"n": 1}], key=lambda d: d["n"]))
