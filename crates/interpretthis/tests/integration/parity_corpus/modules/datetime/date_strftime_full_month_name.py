# Pins `strftime('%B')` — full English month name.
# Expected stdout: `January`.
import datetime
d = datetime.date(2026, 1, 1)
print(d.strftime('%B'))
