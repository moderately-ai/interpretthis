# Pins Match.group() / group(n) for whole-match and indexed groups.
import re
print(re.search(r"\d+", "a1b22").group())
print(re.search(r"(\w)(\d+)", "x42").group(1))
print(re.search(r"(\w)(\d+)", "x42").group(2))
