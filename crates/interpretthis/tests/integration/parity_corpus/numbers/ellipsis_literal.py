# `...` is the distinct Ellipsis singleton, not None. Regression: `...`
# evaluated to None.
print(...)
print(repr(...))
print(type(...).__name__)
print(... is ...)
print(... == ...)
print(... is None)
print(... == None)
print(bool(...))

x = ...
print(x is Ellipsis)         # bare Ellipsis name is the same singleton
print(Ellipsis is ...)
print([1, ..., 3])
print({...: "e"}[...])       # hashable / usable as a key
