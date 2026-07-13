# Pins Match.span() / start() / end() offsets.
import re
print(re.search(r"\d+", "a1b22").span())
print(re.search(r"\d+", "a1b22").start())
print(re.search(r"\d+", "a1b22").end())
