# Pins `strftime('%Y-%m-%d')` for a `date` value.
# Expected stdout: `2026-01-01`.
import datetime
d = datetime.date(2026, 1, 1)
print(d.strftime('%Y-%m-%d'))
