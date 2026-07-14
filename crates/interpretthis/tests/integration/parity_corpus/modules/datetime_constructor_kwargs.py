# datetime constructors accept keyword arguments. Regression: the module call
# path bound _kwargs and dropped them, so timedelta(hours=2) was 0 and
# datetime(..., hour=9) was midnight.
from datetime import datetime, timedelta, date, time

print(timedelta(hours=2))
print(timedelta(days=1, hours=2, minutes=3))
print(timedelta(weeks=1, seconds=30))
print(datetime(2020, 1, 1, hour=9, minute=30))
print(datetime(2020, 1, 1, 9, 30, second=15))
print(date(year=2020, month=6, day=15))
print(time(hour=14, minute=5))

# date.replace / datetime.replace accept keyword components.
print(date(2020, 1, 1).replace(year=2021))
print(date(2020, 1, 1).replace(month=6, day=15))
print(datetime(2020, 1, 1, 9, 30).replace(hour=12))
print(datetime(2020, 1, 1, 9, 30).replace(year=2022, minute=45))

# Unknown keyword and duplicate value raise TypeError.
try:
    timedelta(fortnights=2)
except TypeError:
    print("TypeError")
try:
    datetime(2020, 1, 1, 9, hour=10)
except TypeError:
    print("TypeError")
try:
    date(2020, 1, 1).replace(foo=5)
except TypeError:
    print("TypeError")
