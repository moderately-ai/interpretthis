# datetime.time constructor with hour/minute/second/microsecond.
# Pins Value::Time + time_attribute.
import datetime
t = datetime.time(14, 30, 45, 123456)
print(t.hour)
print(t.minute)
print(t.second)
print(t.microsecond)
# Empty time defaults to midnight.
t0 = datetime.time()
print(t0.hour, t0.minute, t0.second, t0.microsecond)
# isoformat
print(t.isoformat())
print(t0.isoformat())
# strftime
print(t.strftime("%H:%M"))
