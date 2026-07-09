# Pins: datetime.datetime.strptime (classmethod on the type).
# CPython has no module-level datetime.strptime.
from datetime import datetime

dt = datetime.strptime("2026-03-15 14:30:00", "%Y-%m-%d %H:%M:%S")
print(dt.year, dt.month, dt.day, dt.hour, dt.minute, dt.second)

d_only = datetime.strptime("2024-02-29", "%Y-%m-%d")
print(d_only.year, d_only.month, d_only.day, d_only.hour, d_only.minute)

# Attribute bind then call (not only the method-call AST shape).
parse = datetime.strptime
dt2 = parse("2020-01-01", "%Y-%m-%d")
print(dt2.year, dt2.month, dt2.day)

# Nested form: import module, then type.classmethod.
import datetime as dtmod

dt3 = dtmod.datetime.strptime("2019-06-01 08:00:00", "%Y-%m-%d %H:%M:%S")
print(dt3.year, dt3.month, dt3.day, dt3.hour)
