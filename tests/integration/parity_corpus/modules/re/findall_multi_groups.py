# Pins re.findall with multiple groups → list of tuples, per CPython.
import re
print(re.findall(r"(\w)(\d)", "a1b2c3"))
print(re.findall(r"([a-z]+)(\d+)", "abc123 de45"))
