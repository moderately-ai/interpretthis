# A class that defines neither __eq__ nor __hash__ is hashable by identity, so
# its instances work as dict keys / set members and two distinct instances stay
# distinct. Regression: instances routed through value_to_key, which reported
# every instance as "unhashable type".
class Plain:
    def __init__(self, n):
        self.n = n


a = Plain(1)
b = Plain(1)
print(hash(a) == hash(a))       # stable
print(hash(a) == hash(b))       # distinct identities almost never collide -> False
d = {a: "x", b: "y"}
print(len(d))                   # 2 — distinct keys
print(d[a], d[b])
print(a in d, Plain(1) in d)    # True, False (fresh instance not present)
s = {a, b, a}
print(len(s))                   # 2


# Defining __eq__ without __hash__ makes the class unhashable (CPython sets
# __hash__ = None), so it cannot be a dict key.
class Eqable:
    def __init__(self, n):
        self.n = n

    def __eq__(self, other):
        return isinstance(other, Eqable) and self.n == other.n


try:
    hash(Eqable(1))
except TypeError:
    print("unhashable")
