# Pins re.findall with one capture group → list of that group, per CPython.
import re
print(re.findall(r"(\d+)", "a1b22c333"))
