# Pins positional `date.replace(year)` keeping month and day from the original.
# Expected stdout: `2025-01-01`.
import datetime
d = datetime.date(2026, 1, 1)
print(d.replace(2025))
