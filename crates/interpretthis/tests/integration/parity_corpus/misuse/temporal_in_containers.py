# date/time/datetime/timedelta are hashable and orderable; they can be set
# members, dict keys, and be sorted. Equal temporals collapse in a set/dict.
from datetime import date, time, datetime, timedelta

d1 = date(2020, 1, 1)
d2 = date(2020, 1, 1)
d3 = date(2021, 6, 15)
print(len({d1, d2, d3}))
print(d1 in {d2})
print({d1: "a"}[d2])

t1 = time(12, 30)
print(len({t1, time(12, 30), time(9, 0)}))

dt1 = datetime(2020, 1, 1, 12, 0)
print(dt1 in {datetime(2020, 1, 1, 12, 0)})
print({dt1: "x"}[datetime(2020, 1, 1, 12, 0)])

td = timedelta(days=1)
print(len({td, timedelta(hours=24), timedelta(days=2)}))
print(td in {timedelta(seconds=86400)})

# Sorting temporals.
print(sorted([d3, d1, date(2019, 12, 31)]))
print(sorted([timedelta(days=2), timedelta(hours=1), timedelta(minutes=90)]))

# Comparisons.
print(d1 < d3, dt1 <= datetime(2020, 1, 1, 12, 0), td < timedelta(days=2))

# repr of each temporal type differs from str() and is exercised via lists.
from datetime import timezone

print([time(12, 30), time(9, 8, 7, 6), time(12, 30, 0, 500)])
print([datetime(2020, 1, 1, 12, 0), datetime(2020, 1, 1, 12, 30, 45, 678)])
print([timedelta(seconds=3600), timedelta(days=1, seconds=10, microseconds=5), timedelta(0)])
print([timedelta(microseconds=-1)])
print([timezone(timedelta(0)), timezone(timedelta(hours=5, minutes=30))])
print([datetime(2020, 1, 1, 12, 0, tzinfo=timezone(timedelta(0)))])
print(repr(time(0, 0)), repr(datetime(2020, 1, 1)), repr(timedelta(days=2)))
