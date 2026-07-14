# `unhashable in {dict-like}` raises TypeError, it does not answer False.
# Regression: the membership slots did `let Ok(key) = value_to_key(item) else {
# return Ok(false) }`, so an unhashable probe silently reported "not present"
# instead of raising — a membership guard would take the wrong branch.
from collections import Counter, defaultdict

d = {"a": 1}
c = Counter("aab")
dd = defaultdict(int)
dd["x"] = 1

for label, container in [("dict", d), ("counter", c), ("defaultdict", dd)]:
    try:
        [1, 2] in container
        print(label, "NO ERROR")
    except TypeError:
        print(label, "TypeError")

# Hashable membership still answers normally.
print("a" in d)
print("z" in d)
print("a" in c)
print("x" in dd)
