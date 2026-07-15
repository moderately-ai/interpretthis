# date/time/datetime/timedelta hash deterministically in CPython (their hash is
# the hash of the packed state), so a set of them iterates in a reproducible
# hash-table slot order — not insertion order. Print unsorted to pin the order.
from datetime import date, time, datetime, timedelta

print(list({date(2021, 6, 15), date(2020, 1, 1), date(2019, 12, 31), date(2022, 3, 3), date(2018, 7, 7)}))
print(list({timedelta(days=5), timedelta(days=1), timedelta(days=99), timedelta(hours=3), timedelta(microseconds=-1)}))
print(list({time(9, 0), time(23, 30), time(0, 0), time(12, 15), time(6, 6, 6, 6)}))
print(list({datetime(2020, 1, 1, 12, 0), datetime(2019, 5, 5, 5, 5), datetime(2021, 9, 9), datetime(2020, 1, 1, 12, 0, 0, 1)}))

# Operations preserve the table order too.
a = {date(2020, 1, 1), date(2021, 1, 1), date(2022, 1, 1)}
b = {date(2021, 1, 1), date(2023, 1, 1)}
print(list(a | b))
print(list(a & b))
print(list(a - b))
print(list(a ^ b))

# Membership and dedup still hold.
print(date(2020, 1, 1) in a, len({timedelta(hours=24), timedelta(days=1)}))
