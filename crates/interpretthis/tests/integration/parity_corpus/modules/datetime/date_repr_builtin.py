# Pins `repr(d)` rendering as `datetime.date(YYYY, M, D)` with single-space separators.
# Expected stdout: `datetime.date(2026, 1, 1)`.
import datetime
d = datetime.date(2026, 1, 1)
print(repr(d))
