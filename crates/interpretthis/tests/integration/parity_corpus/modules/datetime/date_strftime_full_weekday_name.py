# Pins `strftime('%A')` — full English weekday name.
# 2026-01-01 is a Thursday; expected stdout: `Thursday`.
import datetime
d = datetime.date(2026, 1, 1)
print(d.strftime('%A'))
