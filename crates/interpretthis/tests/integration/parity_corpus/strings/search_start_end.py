# str search methods honour their start/end window and return char indices
# (not byte offsets). Regression: find/rfind/index/count ignored start/end
# entirely, and returned byte offsets for non-ASCII subjects.
s = "abcabcABC"
print(s.find("a", 1), s.find("bc", 3), s.find("z"), s.find("a", 1, 2))
print(s.rfind("a"), s.rfind("a", 0, 3), s.rfind("bc", 0, 4))
print(s.index("A", 3), s.count("a", 1), s.count("bc", 0, 5))
print(s.rindex("bc"), s.rindex("a", 0, 4))

# startswith/endswith honour start/end and accept a tuple of options.
print(s.startswith("bc", 1), s.endswith("BC"), s.endswith("bc", 0, 6))
print(s.startswith(("x", "ab")), s.endswith(("x", "BC")))

# Char indices, not byte offsets, for multi-byte subjects.
h = "héllo"
print(h.find("l"), h.find("l", 4), h.rfind("l"), h.index("o"))

# Negative indices and explicit None defaults.
print("aXbXc".find("X", -3), "abc".find("b", None, None))

# Non-integer bounds and non-str/tuple prefixes raise TypeError.
try:
    "abc".find("b", 1.5)
except TypeError:
    print("TypeError")
try:
    "abc".startswith(5)
except TypeError:
    print("TypeError")

# index/rindex raise ValueError when the substring is outside the window.
try:
    "abcabc".index("a", 4)
except ValueError:
    print("ValueError")
