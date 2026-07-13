# Pins named capture groups: groupdict() and group(name) access.
import re
print(re.search(r"(?P<num>\d+)", "a42").groupdict())
print(re.search(r"(?P<num>\d+)", "a42").group("num"))
