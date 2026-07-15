data = {"users": [{"name": "a", "roles": ["admin"]}, {"name": "b", "roles": ["user", "guest"]}]}
print(data["users"][0]["name"])
print(data["users"][1]["roles"])
print([u["name"] for u in data["users"]])
print({u["name"]: len(u["roles"]) for u in data["users"]})
matrix = [[1,2,3],[4,5,6],[7,8,9]]
print([row[i] for i, row in enumerate(matrix)])
print([[row[c] for row in matrix] for c in range(3)])
nested = [[[1,2],[3,4]],[[5,6],[7,8]]]
print(nested[1][0][1])
print(sum(sum(row) for row in matrix))
config = {"a": {"b": {"c": 42}}}
print(config["a"]["b"]["c"])
tree = {"value": 1, "children": [{"value": 2, "children": []}]}
print(tree["children"][0]["value"])
flat = [x for row in matrix for x in row]
print(flat)
print(list(zip(*matrix)))
d = {}
d.setdefault("k", {}).setdefault("k2", []).append(1)
print(d)
