# Pins `strftime('%j')` — zero-padded 3-digit day of year.
# 2026-03-15 is the 74th day of 2026; expected stdout: `074`.
import datetime
d = datetime.date(2026, 3, 15)
print(d.strftime('%j'))
