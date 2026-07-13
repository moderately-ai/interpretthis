# Pins truthiness of re.search/match results in boolean context.
import re
print("yes" if re.search(r"\d", "a1") else "no")
print("yes" if re.search(r"\d", "abc") else "no")
print("yes" if re.match(r"\d", "1abc") else "no")
