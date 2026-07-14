# Set operations over user instances must treat distinct objects as distinct.
# Regression: union/intersection/difference/symmetric_difference keyed every
# element through `value_to_key(x).ok()`, which is None for an instance, so all
# instances collapsed into a single element and the algebra returned nonsense.
class Item:
    def __init__(self, name):
        self.name = name

    def __repr__(self):
        return f"Item({self.name!r})"


a = Item("a")
b = Item("b")
c = Item("c")

s1 = {a, b}
s2 = {b, c}

# len is preserved: two distinct instances stay two elements.
print(len(s1), len(s2))
print(a in s1, c in s1)

# Difference keeps `a`, drops `b`.
diff = s1 - s2
print([x.name for x in diff])

# Intersection keeps only `b`.
inter = s1 & s2
print([x.name for x in inter])

# Union has all three.
union = s1 | s2
print(sorted(x.name for x in union))

# Symmetric difference: a and c.
sym = s1 ^ s2
print(sorted(x.name for x in sym))

# Methods agree with operators.
print(sorted(x.name for x in s1.union(s2)))
print([x.name for x in s1.difference(s2)])
print(s1.isdisjoint({c}), s1.isdisjoint(s2))
