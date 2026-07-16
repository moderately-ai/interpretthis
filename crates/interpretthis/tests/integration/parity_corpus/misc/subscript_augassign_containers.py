from collections import Counter, defaultdict, deque, OrderedDict
c = Counter()
c["a"] += 1
c["a"] += 2
c["b"] += 5
print(dict(c))
c["a"] -= 1
print(c["a"])
c["new"] = 10
print(c["new"])
dd = defaultdict(int)
dd["x"] += 1
dd["x"] += 1
dd["y"] += 5
print(dict(dd))
dd2 = defaultdict(list)
dd2["a"] += [1, 2]
dd2["a"] += [3]
print(dict(dd2))
od = OrderedDict()
od["a"] = 1
od["a"] += 10
print(od["a"])
d = {"count": 0}
d["count"] += 1
d["count"] += 5
print(d["count"])
d.setdefault("list", []).append(1)
d.setdefault("list", []).append(2)
print(d["list"])
lst = [0, 0, 0]
lst[1] += 10
lst[0] += 5
print(lst)
nested = {"a": {"b": 0}}
nested["a"]["b"] += 100
print(nested)
matrix = [[0, 0], [0, 0]]
matrix[0][1] += 5
matrix[1][0] += 3
print(matrix)
counts = {}
for word in "a b a c a b".split():
    counts[word] = counts.get(word, 0) + 1
print(sorted(counts.items()))
grid = defaultdict(lambda: defaultdict(int))
grid[0][0] += 1
grid[0][1] += 2
grid[1][0] += 3
print({k: dict(v) for k, v in grid.items()})
scores = Counter(a=1, b=2)
scores["a"] += 10
scores["c"] += 5
print(dict(scores))
freq = defaultdict(int)
text = "hello world"
for char in text:
    freq[char] += 1
print(sorted(freq.items()))
dq = deque([1, 2, 3])
dq[0] = 10
dq[1] += 5
print(list(dq))
data = {"items": []}
data["items"].append("x")
data["items"] += ["y", "z"]
print(data)
tallies = Counter()
for x in [1, 1, 2, 3, 3, 3]:
    tallies[x] += 1
print(tallies.most_common())
nested_dd = defaultdict(lambda: {"count": 0, "items": []})
nested_dd["a"]["count"] += 1
nested_dd["a"]["items"].append("x")
print(dict(nested_dd["a"]))
matrix2 = [[0] * 3 for _ in range(3)]
for i in range(3):
    matrix2[i][i] += 1
print(matrix2)
