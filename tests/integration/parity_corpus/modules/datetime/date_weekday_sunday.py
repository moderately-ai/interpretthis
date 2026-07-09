# Pins the Sunday boundary of `.weekday()` (Sun=6).
# 2026-01-11 is a Sunday; expected stdout: `6`.
import datetime
d = datetime.date(2026, 1, 11)
print(d.weekday())
