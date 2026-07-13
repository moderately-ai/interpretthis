# Pins that constructing `datetime.date(_, 2, 30)` raises (day out of range for month).
# Expected: non-zero exit from both engines, no stdout.
import datetime
d = datetime.date(2026, 2, 30)
