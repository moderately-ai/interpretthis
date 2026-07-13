# Pins re.split across whitespace, comma, and bounded-maxsplit forms.
import re
print(re.split(r"\s+", "a b  c   d"))
print(re.split(r",\s*", "a, b,c,  d"))
print(re.split(r"\d", "no digits"))
print(re.split(r",", "a,b,c", 1))
print(re.split(r",", ",a,b,"))
