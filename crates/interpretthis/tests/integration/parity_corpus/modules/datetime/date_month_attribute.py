# Pins `datetime.date(y, m, d).month` echoing the month passed to the constructor.
# Expected stdout: `7`.
import datetime
d = datetime.date(2026, 7, 4)
print(d.month)
