# Pins positional `date.replace(year, month, day)` rebuilding the date wholesale.
# Expected stdout: `2027-03-15`.
import datetime
d = datetime.date(2026, 1, 1)
print(d.replace(2027, 3, 15))
