# set() and {..for..} raise on an unhashable element and dedup instances by
# __eq__. Regression: both open-coded a `value_to_key(x).ok()` dedup, so every
# instance collapsed to one (all keyed as None) and unhashables were silently
# dropped instead of raising.
try:
    set([[1], [2]])
    print("set() NO ERROR")
except TypeError:
    print("set() TypeError")

try:
    {list(x) for x in [[1], [2], [3]]}
    print("comp NO ERROR")
except TypeError:
    print("comp TypeError")

# set() and set-comp dedup normally on hashable elements.
print(sorted(set([1, 2, 2, 3, 1])))
print(sorted({x % 3 for x in range(10)}))


# Distinct instances (default identity hash/eq) must NOT collapse to one.
class C:
    def __init__(self, v):
        self.v = v


a, b = C(1), C(2)
print(len(set([a, b, a])))
print(len({c for c in [a, b]}))
