# Pins `date.isoformat()` rendering as `YYYY-MM-DD`.
# Expected stdout: `2026-03-15`.
import datetime
d = datetime.date(2026, 3, 15)
print(d.isoformat())
