# Pins leap-year handling — 2024 is a leap year, so Feb 29 is a valid `date`.
# Expected stdout: `2024-02-29`.
import datetime
d = datetime.date(2024, 2, 29)
print(d.isoformat())
