# date +/- timedelta uses the timedelta's floored whole-day count (its .days),
# not truncation toward zero. Regression: a negative sub-day delta truncated to
# 0 days and left the date unchanged.
from datetime import date, timedelta

print(date(2020, 1, 10) + timedelta(hours=-1))     # -1h -> floor -1 day
print(date(2020, 1, 10) - timedelta(hours=1))      # +1h .days == 0 -> unchanged
print(date(2020, 1, 10) - timedelta(hours=-1))     # -(-1 day) -> +1 day
print(date(2020, 1, 10) + timedelta(hours=25))     # +1 day
print(date(2020, 1, 10) + timedelta(days=1, hours=1))
print(date(2020, 1, 10) - timedelta(days=1, hours=1))
print(date(2020, 1, 10) + timedelta(minutes=-1))   # tiny negative -> -1 day
