# bytes search methods honour start/end, accept an int (byte value) needle,
# and expose rfind/index/rindex/count. Regression: find ignored start/end and
# returned -1 on a too-large offset; rfind/index/rindex/count did not exist;
# startswith/endswith ignored start/end and rejected a tuple of prefixes.
b = b"abcabcABC"
print(b.find(b"a", 1), b.rfind(b"a"), b.rfind(b"a", 0, 3))
print(b.index(b"bc", 3), b.count(b"a"), b.count(b"a", 1))
print(b.find(97), b.find(97, 1))            # int needle = byte value
print(b.startswith(b"bc", 1), b.endswith(b"BC"), b.endswith(b"bc", 0, 6))
print(b.startswith((b"x", b"ab")), b.endswith((b"x", b"BC")))
print(b.find(b"z"), b.find(b""), b.rfind(b""))
print(b.rindex(b"a"), b.rindex(b"bc", 0, 5))

# Not found / bad arguments raise the right exception type.
try:
    b.index(b"z")
except ValueError:
    print("ValueError")
try:
    b.rindex(b"z")
except ValueError:
    print("ValueError")
try:
    b.find(300)                             # byte out of range(0, 256)
except ValueError:
    print("ValueError")
try:
    b.find("a")                             # str needle
except TypeError:
    print("TypeError")
try:
    b.startswith(5)                         # int prefix
except TypeError:
    print("TypeError")
try:
    b.find(b"a", 1.5)                        # float bound
except TypeError:
    print("TypeError")
