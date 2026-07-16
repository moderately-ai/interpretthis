from collections import Counter, defaultdict, OrderedDict, deque
d = {"a": 1, "b": 2, "c": 3}
del d["b"]
print(d)
c = Counter("aabbcc")
del c["a"]
print(dict(c))
od = OrderedDict([("x", 1), ("y", 2), ("z", 3)])
del od["y"]
print(list(od.items()))
dd = defaultdict(int)
dd["a"] = 1
dd["b"] = 2
del dd["a"]
print(dict(dd))
lst = [1, 2, 3, 4, 5]
del lst[2]
print(lst)
del lst[0]
print(lst)
del lst[-1]
print(lst)
nested = {"a": {"b": 1, "c": 2}}
del nested["a"]["b"]
print(nested)
matrix = [[1, 2, 3], [4, 5, 6]]
del matrix[0][1]
print(matrix)
d2 = {"keep": 1, "remove": 2}
if "remove" in d2:
    del d2["remove"]
print(d2)
data = {i: i**2 for i in range(5)}
for k in [1, 3]:
    del data[k]
print(data)
lst2 = list(range(10))
del lst2[2:5]
print(lst2)
del lst2[::2]
print(lst2)
dq = deque([1, 2, 3, 4])
del dq[1]
print(list(dq))
config = {"a": {"nested": {"deep": "value"}}}
del config["a"]["nested"]["deep"]
print(config)
try:
    d3 = {"x": 1}
    del d3["missing"]
except KeyError as e:
    print("keyerror:", str(e))
mapping = dict.fromkeys(range(5), 0)
del mapping[2]
print(sorted(mapping.keys()))
s = {1, 2, 3}
s.discard(2)
print(sorted(s))
s.remove(1)
print(sorted(s))
counters = Counter(a=5, b=3, c=1)
del counters["b"]
print(dict(counters))
