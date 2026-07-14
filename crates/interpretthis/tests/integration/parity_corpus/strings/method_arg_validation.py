# str methods validate their arguments instead of silently coercing to a
# default. Regression: strip(non-str) fell back to whitespace, split/replace
# swallowed a bad maxsplit/count into "unlimited", and split("") did not raise.

# strip family: None keeps the whitespace default; a non-str/non-None raises.
print(repr("  x  ".strip(None)))
print(repr("xxhixx".strip("x")))
try:
    "  x  ".strip(5)
except TypeError:
    print("TypeError")
try:
    "y".lstrip(1.5)
except TypeError:
    print("TypeError")
try:
    "y".rstrip([])
except TypeError:
    print("TypeError")

# split/rsplit maxsplit must be an integer; empty separator is a ValueError.
print("a b c".split(" ", True))  # bool is an int -> maxsplit 1
try:
    "a b".split("x", "y")
except TypeError:
    print("TypeError")
try:
    "a,b,c".split(",", 1.5)
except TypeError:
    print("TypeError")
try:
    "a b".rsplit("x", "y")
except TypeError:
    print("TypeError")
try:
    "abc".split("")
except ValueError:
    print("ValueError")
try:
    "abc".rsplit("")
except ValueError:
    print("ValueError")

# replace count must be an integer.
print("aXbXc".replace("X", "-", True))
try:
    "aaa".replace("a", "b", 1.5)
except TypeError:
    print("TypeError")
