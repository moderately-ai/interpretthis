# Pins positional `date.replace(year, month)` keeping day from the original.
# Expected stdout: `2030-06-01`.
import datetime
d = datetime.date(2026, 1, 1)
print(d.replace(2030, 6))
