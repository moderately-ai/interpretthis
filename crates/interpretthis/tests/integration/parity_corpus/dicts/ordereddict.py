from collections import OrderedDict

od = OrderedDict([("a", 1), ("b", 2), ("c", 3)])
# Distinct repr and type.
print(od)
print(repr(od))
print([od])
print(type(od).__name__)
print(OrderedDict())

# Dict behaviours.
print(od["a"], od.get("z", -1), len(od))
print("a" in od, "z" in od)
print(list(od), list(od.keys()), list(od.values()), list(od.items()))
od["d"] = 4
od.move_to_end("a")
print(list(od.keys()))
del od["b"]
print(list(od.items()))
print(list(reversed(OrderedDict([("a", 1), ("b", 2)]))))

# Dict-subclass isinstance.
print(isinstance(od, dict), isinstance(od, OrderedDict))

# Order-sensitive equality between OrderedDicts; unordered against a plain dict.
o1 = OrderedDict([("a", 1), ("b", 2)])
o2 = OrderedDict([("b", 2), ("a", 1)])
o3 = OrderedDict([("a", 1), ("b", 2)])
print(o1 == o2, o1 == o3)
print(o1 == {"b": 2, "a": 1}, {"b": 2, "a": 1} == o1)

# dict() copy, ** unpacking (literal + call), merge operator types.
print(dict(o1))
print({**o1, "c": 3})


def f(**kw):
    return sorted(kw.items())


print(f(**o1))
print(o1 | OrderedDict([("c", 3)]))
print(type(o1 | {"c": 3}).__name__, type({"c": 3} | o1).__name__)

# Reference semantics (aliasing shares the store).
alias = od
alias["e"] = 5
print("e" in od)

# Comprehension over items, and JSON.
print({k: v * 10 for k, v in o1.items()})
import json

print(json.dumps(OrderedDict([("x", 1), ("y", 2)])))

# copy / deepcopy preserve the type.
import copy

print(type(copy.copy(o1)).__name__, type(copy.deepcopy(o1)).__name__)

# popitem / setdefault / update, truthiness.
o4 = OrderedDict([("p", 1)])
o4.update({"q": 2})
o4.setdefault("r", 3)
print(list(o4.items()))
print(o4.popitem())
print(bool(OrderedDict()), bool(o1))
