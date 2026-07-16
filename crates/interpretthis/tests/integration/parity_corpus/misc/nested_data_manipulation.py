data = {
    "users": [
        {"name": "Alice", "roles": ["admin", "user"], "age": 30},
        {"name": "Bob", "roles": ["user"], "age": 25},
    ],
    "count": 2,
}
print(data["users"][0]["name"])
print(data["users"][1]["roles"])
print([u["name"] for u in data["users"]])
print([u["name"] for u in data["users"] if "admin" in u["roles"]])
print(sum(u["age"] for u in data["users"]))
print(max(data["users"], key=lambda u: u["age"])["name"])
print(sorted(data["users"], key=lambda u: u["age"])[0]["name"])
data["users"].append({"name": "Carol", "roles": ["guest"], "age": 35})
print(len(data["users"]))
data["users"][0]["age"] += 1
print(data["users"][0]["age"])
data["users"][0]["roles"].append("superuser")
print(data["users"][0]["roles"])
tree = {"value": 1, "children": [{"value": 2, "children": []}, {"value": 3, "children": [{"value": 4, "children": []}]}]}
def sum_tree(node):
    return node["value"] + sum(sum_tree(c) for c in node["children"])
print(sum_tree(tree))
def count_nodes(node):
    return 1 + sum(count_nodes(c) for c in node["children"])
print(count_nodes(tree))
grades = {"Alice": [90, 85, 95], "Bob": [70, 75, 80]}
averages = {name: sum(scores) / len(scores) for name, scores in grades.items()}
print(averages)
print(max(averages, key=averages.get))
inventory = {"apple": 50, "banana": 30, "cherry": 100}
low_stock = {k: v for k, v in inventory.items() if v < 60}
print(sorted(low_stock.items()))
total = sum(inventory.values())
print(total)
matrix = [[1, 2, 3], [4, 5, 6], [7, 8, 9]]
print([[cell * 2 for cell in row] for row in matrix])
print([[matrix[i][j] for i in range(3)] for j in range(3)])
diagonal = [matrix[i][i] for i in range(3)]
print(diagonal, sum(diagonal))
records = [
    {"id": 1, "tags": ["a", "b"]},
    {"id": 2, "tags": ["b", "c"]},
    {"id": 3, "tags": ["a", "c"]},
]
all_tags = set()
for r in records:
    all_tags.update(r["tags"])
print(sorted(all_tags))
from collections import defaultdict
tag_to_ids = defaultdict(list)
for r in records:
    for tag in r["tags"]:
        tag_to_ids[tag].append(r["id"])
print({k: v for k, v in sorted(tag_to_ids.items())})
config = {"db": {"host": "localhost", "port": 5432, "opts": {"ssl": True}}}
print(config["db"]["opts"]["ssl"])
config["db"]["opts"]["timeout"] = 30
print(sorted(config["db"]["opts"].items()))
nested_list = [[[1, 2], [3, 4]], [[5, 6], [7, 8]]]
flat = [x for a in nested_list for b in a for x in b]
print(flat, sum(flat))
d = {}
d.setdefault("list", []).append(1)
d.setdefault("list", []).append(2)
print(d)
scores = {"math": 90, "science": 85}
scores["math"] = scores.get("math", 0) + 5
print(scores)
groups = {}
for item in [("fruit", "apple"), ("veg", "carrot"), ("fruit", "banana")]:
    groups.setdefault(item[0], []).append(item[1])
print({k: sorted(v) for k, v in sorted(groups.items())})
