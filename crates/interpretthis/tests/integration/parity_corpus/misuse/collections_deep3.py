from collections import Counter, OrderedDict, deque, defaultdict, ChainMap, namedtuple

c = Counter("abracadabra")
print(c.most_common(2), c["a"], c["z"])
print(sorted((c + Counter("aaa")).items()), sorted((c - Counter("aa")).items()))
print(sorted((c & Counter("aabbb")).items()), sorted((c | Counter("zzz")).items()))
c.update("aa")
print(c["a"])

od = OrderedDict([("a", 1), ("b", 2), ("c", 3)])
od.move_to_end("a")
print(list(od.items()))
od.move_to_end("c", last=False)
print(list(od.keys()))
print(od.popitem(), od.popitem(last=False))

dq = deque([1, 2, 3], maxlen=5)
dq.appendleft(0)
dq.append(4)
dq.extend([5, 6])
print(list(dq))
dq.rotate(2)
print(list(dq))
print(dq.pop(), dq.popleft(), list(dq))

dd = defaultdict(list)
dd["x"].append(1)
dd["x"].append(2)
dd["y"].append(3)
print(dict(dd))

cm = ChainMap({"a": 1}, {"a": 2, "b": 3})
print(cm["a"], cm["b"], dict(cm))

Pt = namedtuple("Pt", ["x", "y"])
p = Pt(1, 2)
print(p.x, p.y, p._asdict(), p._replace(x=10), list(p))

nested = defaultdict(lambda: defaultdict(int))
nested["a"]["b"] += 5
print(dict((k, dict(v)) for k, v in nested.items()))
