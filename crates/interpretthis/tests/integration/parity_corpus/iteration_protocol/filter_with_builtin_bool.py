# Pins: `filter(bool, items)` — builtin `bool` as the predicate. This is
# the canonical "drop falsy values" idiom in Python.
print(list(filter(bool, [0, 1, "", "x", False, True])))
