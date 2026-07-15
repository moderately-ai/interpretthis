# datetime.timezone.utc is a class constant equal to timezone(timedelta(0)).
from datetime import datetime, timezone, timedelta

print(timezone.utc)
print(repr(timezone.utc))
print(timezone.utc == timezone(timedelta(0)))

dt = datetime(2020, 1, 1, 12, 0, tzinfo=timezone.utc)
print(dt)
print(repr(dt))
print(dt.tzinfo == timezone.utc)

# Access through a bound name too.
tz = timezone.utc
print([tz])
print(datetime(2021, 6, 15, tzinfo=tz).isoformat())
