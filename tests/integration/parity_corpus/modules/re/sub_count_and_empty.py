# Pins re.sub count cap and empty-replacement deletion semantics.
import re
print(re.sub(r"\d", "X", "1234", count=2))
print(re.sub(r"\s", "", "a b c d"))
