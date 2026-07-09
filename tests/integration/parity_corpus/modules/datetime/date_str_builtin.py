# Pins the `str(d)` builtin returning ISO `YYYY-MM-DD` for a `date` value.
# Expected stdout: `2026-01-01`.
import datetime
d = datetime.date(2026, 1, 1)
print(str(d))
