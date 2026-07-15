import copy
a = [1, [2, 3], 4]
b = copy.copy(a)
b[0] = 99
print(a[0], b[0])
b[1][0] = 88
print(a[1][0], b[1][0])
c = copy.deepcopy(a)
c[1][0] = 77
print(a[1][0], c[1][0])
d = {"x": [1, 2], "y": 3}
e = copy.deepcopy(d)
e["x"].append(99)
print(d["x"], e["x"])
import copy
t = ([1, 2], [3, 4])
tc = copy.deepcopy(t)
tc[0].append(5)
print(t[0], tc[0])
