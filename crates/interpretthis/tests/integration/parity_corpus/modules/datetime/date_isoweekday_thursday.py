# Pins Python's ISO Mon=1..Sun=7 convention via `.isoweekday()`.
# 2026-01-01 is a Thursday; expected stdout: `4`.
import datetime
d = datetime.date(2026, 1, 1)
print(d.isoweekday())
