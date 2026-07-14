import re
print(re.split(r"(\d)", "a1b2c"))
print(re.findall(r"\d+", "a12b345"))
print(re.sub(r"\d", "X", "a1b2", count=1))
m = re.match(r"(\w+)@(\w+)", "user@host")
print(m.group(1), m.group(2))
