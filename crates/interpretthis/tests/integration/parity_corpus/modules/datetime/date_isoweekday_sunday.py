# Pins the Sunday boundary of `.isoweekday()` (ISO Sun=7).
# 2026-01-11 is a Sunday; expected stdout: `7`.
import datetime
d = datetime.date(2026, 1, 11)
print(d.isoweekday())
