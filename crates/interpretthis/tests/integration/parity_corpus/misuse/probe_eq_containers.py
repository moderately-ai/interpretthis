class Key:
    def __init__(self, name): self.name = name
    def __eq__(self, o): return isinstance(o, Key) and self.name == o.name
    def __hash__(self): return hash(self.name)
    def __repr__(self): return f"K({self.name})"
d = {Key("a"): 1, Key("b"): 2}
print(d[Key("a")])
print(Key("a") in d)
print(Key("c") in d)
d[Key("a")] = 10
print(d[Key("a")], len(d))
s = {Key("x"), Key("y"), Key("x")}
print(len(s))
print(Key("x") in s)
keys = [Key("a"), Key("b"), Key("a")]
print(len(set(keys)))
print(sorted(set([Key("m"), Key("n"), Key("m")]), key=lambda k: k.name))
from collections import Counter
c = Counter([Key("a"), Key("a"), Key("b")])
print(c[Key("a")])
d2 = dict.fromkeys([Key("p"), Key("q")], 0)
print(len(d2))
