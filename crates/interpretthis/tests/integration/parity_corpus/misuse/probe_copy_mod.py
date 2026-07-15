import copy
original = [1, [2, 3], {"a": 4}]
shallow = copy.copy(original)
deep = copy.deepcopy(original)
original[1].append(99)
print(shallow[1])
print(deep[1])
d = {"x": [1, 2], "y": {"z": 3}}
d2 = copy.deepcopy(d)
d["x"].append(3)
print(d2["x"])
lst = [[1,2],[3,4]]
copied = copy.deepcopy(lst)
copied[0][0] = 99
print(lst[0][0])
print(copy.copy((1,2,3)))
print(copy.deepcopy({1,2,3}) == {1,2,3})
nested = {"a": {"b": {"c": [1,2,3]}}}
nc = copy.deepcopy(nested)
nc["a"]["b"]["c"].append(4)
print(nested["a"]["b"]["c"])
t = ([1,2], [3,4])
tc = copy.deepcopy(t)
tc[0].append(5)
print(t[0])
