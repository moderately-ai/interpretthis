# Pins `datetime.date(y, m, d).day` echoing the day passed to the constructor.
# Expected stdout: `4`.
import datetime
d = datetime.date(2026, 7, 4)
print(d.day)
