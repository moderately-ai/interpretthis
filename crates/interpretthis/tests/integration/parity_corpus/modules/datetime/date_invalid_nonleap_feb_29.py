# Pins that Feb 29 of a non-leap year raises — 2026 is not a leap year.
# Expected: non-zero exit from both engines, no stdout.
import datetime
d = datetime.date(2026, 2, 29)
