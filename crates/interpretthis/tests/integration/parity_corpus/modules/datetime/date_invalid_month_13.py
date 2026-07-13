# Pins that constructing `datetime.date(_, 13, _)` raises (month out of range).
# Expected: non-zero exit from both engines, no stdout.
import datetime
d = datetime.date(2026, 13, 1)
