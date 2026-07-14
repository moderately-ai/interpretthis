# `re.compile` returns a reusable compiled pattern whose methods mirror the
# module-level functions with the pattern bound. Regression: `re.compile` was
# absent entirely (AttributeError on the module).
import re

pat = re.compile(r"(\d+)-(\d+)")
print(repr(pat))
print(pat.pattern)

m = pat.search("range 10-20 here")
print(m.group(0), m.group(1), m.group(2))
print(pat.match("10-20 leading").group(0))
print(pat.match("no leading number") is None)
print(pat.findall("1-2 and 3-4 and 5-6"))

word = re.compile(r"\w+")
print(word.findall("the quick brown fox"))
print(word.sub("X", "a b c"))

comma = re.compile(r"\s*,\s*")
print(comma.split("a,  b ,c ,  d"))

# A bad pattern raises re.error at compile time, not at first use.
try:
    re.compile("(")
except re.error as e:
    print("bad pattern:", type(e).__name__)

# Compiled patterns compare equal when built from the same source.
print(re.compile("abc") == re.compile("abc"), re.compile("abc") == re.compile("abd"))
