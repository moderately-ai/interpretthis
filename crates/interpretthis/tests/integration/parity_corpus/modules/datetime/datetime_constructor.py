# datetime.datetime constructor: year/month/day required, time
# components optional. Pins Value::DateTime + datetime_attribute.
import datetime
dt = datetime.datetime(2026, 1, 15, 14, 30, 45, 123456)
print(dt.year)
print(dt.month)
print(dt.day)
print(dt.hour)
print(dt.minute)
print(dt.second)
print(dt.microsecond)
# Defaults: just date components.
d2 = datetime.datetime(2026, 6, 1)
print(d2.year, d2.month, d2.day, d2.hour, d2.minute, d2.second)
# isoformat with microseconds present
print(dt.isoformat())
# isoformat without microseconds
print(d2.isoformat())
# strftime
print(dt.strftime("%Y/%m/%d %H:%M"))
