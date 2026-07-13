# Pins `datetime.date(y, m, d).year` echoing the year passed to the constructor.
# Expected stdout: `2026`.
import datetime
d = datetime.date(2026, 1, 1)
print(d.year)
