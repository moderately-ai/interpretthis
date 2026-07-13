# Pins Match.groups() returning the tuple of captured groups.
import re
print(re.search(r"(\w)(\d+)", "x42").groups())
