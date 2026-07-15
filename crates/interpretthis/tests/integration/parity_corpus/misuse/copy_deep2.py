import copy

a = [1, [2, 3], {"k": [4, 5]}]
b = copy.copy(a)
c = copy.deepcopy(a)
a[1].append(99)
print(b[1], c[1])
a[2]["k"].append(88)
print(b[2], c[2])

d = {"list": [1, 2], "nested": {"x": [3, 4]}}
dd = copy.deepcopy(d)
d["list"].append(3)
d["nested"]["x"].append(5)
print(dd["list"], dd["nested"]["x"])

# copy of tuples, sets
t = (1, [2, 3])
tc = copy.deepcopy(t)
t[1].append(4)
print(tc[1])

s = {1, 2, 3}
sc = copy.copy(s)
sc.add(4)
print(sorted(s), sorted(sc))

# deepcopy with cycles
lst = [1, 2]
lst.append(lst)
lc = copy.deepcopy(lst)
print(lc[0], lc[1], lc[2] is lc)
