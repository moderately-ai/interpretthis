# Pins re.sub with literal replacement strings against CPython.
import re
print(re.sub(r"\d+", "X", "a1b22c333"))
print(re.sub(r"[aeiou]", "*", "hello world"))
print(re.sub(r"\d+", "X", "no digits"))
