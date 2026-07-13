# Pins the printed repr of re.search results (Match object vs None) against CPython.
import re
print(re.search(r"\d+", "a1b22"))
print(re.search(r"xyz", "abc"))
