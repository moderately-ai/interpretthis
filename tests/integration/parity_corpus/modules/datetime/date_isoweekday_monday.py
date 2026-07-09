# Pins the Monday boundary of `.isoweekday()` (ISO Mon=1).
# 2026-01-05 is a Monday; expected stdout: `1`.
import datetime
d = datetime.date(2026, 1, 5)
print(d.isoweekday())
