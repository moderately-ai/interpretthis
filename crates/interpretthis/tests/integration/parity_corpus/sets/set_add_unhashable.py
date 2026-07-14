# set.add(unhashable) raises TypeError. Regression: add keyed the element through
# `value_to_key(x).ok()`, so an unhashable element (list/dict/set) was silently
# added to the set.
s = set()
try:
    s.add([1, 2])
    print("NO ERROR", s)
except TypeError:
    print("TypeError")

try:
    s.add({1: 2})
    print("NO ERROR", s)
except TypeError:
    print("TypeError")

# Hashable adds still work and dedup.
s.add(5)
s.add(5)
s.add((1, 2))
print(sorted(s, key=repr))
