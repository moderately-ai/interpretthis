lst = [3, 1, 4, 1, 5, 9, 2, 6]
lst.insert(0, 0)
print(lst)
lst.insert(-1, 99)
print(lst)
lst.insert(100, 100)
print(lst)
print(lst.pop(), lst.pop(0), lst.pop(-1))
lst.remove(1)
print(lst)
print(lst.index(4), lst.count(1))
lst.reverse()
print(lst)
lst.extend([7, 8])
print(lst)
lst[2:4] = [20, 30, 40]
print(lst)
lst.clear()
print(lst)
a = [1, 2, 3]
b = a
b.append(4)
print(a, a is b)
c = a.copy()
c.append(5)
print(a, c)
data = [(3, "c"), (1, "a"), (2, "b")]
data.sort()
print(data)
data.sort(key=lambda x: x[1], reverse=True)
print(data)
words = ["banana", "apple", "cherry"]
words.sort(key=len)
print(words)
nums = [5, 2, 8, 1, 9]
print(sorted(nums), nums)
mixed = [[1, 2], [3], [4, 5, 6]]
mixed.sort(key=len)
print(mixed)
d = {"a": 1, "b": 2, "c": 3}
print(d.get("a"), d.get("z"), d.get("z", -1))
d.setdefault("d", 4)
d.setdefault("a", 99)
print(d)
print(d.pop("a"), d.pop("z", None))
print(d.popitem())
d.update({"x": 10}, y=20)
print(sorted(d.items()))
print(list(d.keys()), list(d.values()))
d2 = dict.fromkeys(["p", "q"], 0)
print(d2)
nested = {"a": {"b": [1, 2, {"c": 3}]}}
print(nested["a"]["b"][2]["c"])
d3 = {i: i**2 for i in range(5)}
print(d3)
print({v: k for k, v in {"a": 1, "b": 2}.items()})
print(len(d), "x" in d, "z" not in d)
inv = {}
for k, v in [("a", 1), ("b", 2), ("a", 3)]:
    inv.setdefault(v, []).append(k)
print(inv)
counts = {}
for c in "hello":
    counts[c] = counts.get(c, 0) + 1
print(counts)
