# The builtin hash() must reproduce CPython's values bit-for-bit under
# PYTHONHASHSEED=0 (the parity harness sets it) for str/bytes/tuple/frozenset,
# not just for the numeric tower. Regression for hash() routing through the
# CPython-exact pyhash port instead of Rust's DefaultHasher.
print(hash("hello"))
print(hash(""))
print(hash("a longer string with unicode: café ☃"))
print(hash(b"bytes here"))
print(hash(b""))
print(hash(()))
print(hash((1, 2, 3)))
print(hash((1,)))
print(hash(("a", "b", ("nested", 42))))
print(hash((True, False, None)))
print(hash(frozenset([1, 2, 3])) == hash(frozenset([3, 2, 1])))
print(hash(frozenset()) )
print(hash("hello") == hash("hello"))
print(hash((1, 2)) == hash((1, 2)))
# equal-across-types keys still share a slot (numeric tower preserved)
print(hash(1) == hash(1.0) == hash(True))
d = {("k", 1): "a"}
print(d[("k", 1)])
s = {"x", "y", "x"}
print(len(s))
