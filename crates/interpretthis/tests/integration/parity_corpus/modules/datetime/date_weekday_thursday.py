# Pins Python's Mon=0..Sun=6 convention via `.weekday()`.
# 2026-01-01 is a Thursday; expected stdout: `3`.
import datetime
d = datetime.date(2026, 1, 1)
print(d.weekday())
