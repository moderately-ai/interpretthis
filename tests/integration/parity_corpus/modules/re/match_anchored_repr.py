# Pins the printed repr of re.match (anchored at start) and fullmatch results.
import re
print(re.match(r"\d+", "123abc"))
print(re.match(r"\d+", "abc123"))
print(re.fullmatch(r"\d+", "123"))
print(re.fullmatch(r"\d+", "12a"))
