# Pins the Monday boundary of `.weekday()` (Mon=0).
# 2026-01-05 is a Monday; expected stdout: `0`.
import datetime
d = datetime.date(2026, 1, 5)
print(d.weekday())
