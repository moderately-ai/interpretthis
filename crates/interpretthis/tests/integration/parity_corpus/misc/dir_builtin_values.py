# dir() of a builtin value returns the exact sorted attribute list CPython does
# (object dunders + the type's own dunders + methods + data attributes, plus the
# universal __class__ name, whose read aliases type(x)). Only builtin *values*
# are supported — the no-arg / instance / module forms stay blocked for security.
for v in [5, [], "s", {}, (1,), set(), 3.14, frozenset(), b"x", range(3), True, None, 1 + 2j, bytearray()]:
    print(dir(v))
print("append" in dir([]), "__class__" in dir(5), "fromkeys" in dir({}))
print("is_integer" in dir(5), "format_map" in dir("s"), "maketrans" in dir(b""))
print("start" in dir(range(3)), "real" in dir(5), "imag" in dir(1 + 2j))
print(len(dir([])) == len(set(dir([]))))  # sorted + de-duplicated
print(dir([]) == sorted(dir([])))
