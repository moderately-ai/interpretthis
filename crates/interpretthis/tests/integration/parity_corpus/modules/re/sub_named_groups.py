# Pins: re.sub with \N backref; named groups via (?P<name>...);
# .groupdict() returns the name->match map in insertion order.
# Heavy customer pattern for data extraction.
import re

print(re.sub(r"(\w+) (\w+)", r"\2 \1", "John Smith"))

m = re.search(r"(?P<year>\d{4})-(?P<month>\d{2})", "2025-01-15")
if m:
    print(m.group("year"))
    print(m.group("month"))
    print(m.groupdict())

print(re.findall(r"\d+", "abc 123 def 456"))
print(re.split(r"\s+", "  hello   world  "))
