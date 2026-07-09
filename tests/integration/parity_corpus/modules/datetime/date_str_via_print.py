# Pins `print(d)` calling `date.__str__`, which yields ISO `YYYY-MM-DD`.
# Expected stdout: `2026-01-01`.
import datetime
d = datetime.date(2026, 1, 1)
print(d)
