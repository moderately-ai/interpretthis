# timedelta construction + arithmetic with date / datetime. Pins
# Value::TimeDelta + datetime::try_arith.
import datetime
td = datetime.timedelta(1, 0, 0)         # 1 day, 0 seconds, 0 microseconds
print(td.days, td.seconds, td.microseconds)
print(td)                                # "1 day, 0:00:00"
# date + timedelta
d = datetime.date(2026, 1, 15)
print(d + td)                            # 2026-01-16
print(d + datetime.timedelta(0, 0, 0, 0, 0, 0, 1))  # +1 week -> 2026-01-22
# datetime + timedelta
dt = datetime.datetime(2026, 1, 15, 14, 30, 0)
print(dt + datetime.timedelta(0, 3600))  # +1 hour
# datetime - datetime -> timedelta
dt2 = datetime.datetime(2026, 1, 16, 14, 30, 0)
diff = dt2 - dt
print(diff.days, diff.seconds, diff.microseconds)
# timedelta + timedelta
print(datetime.timedelta(0, 30) + datetime.timedelta(0, 45))
# total_seconds
print(datetime.timedelta(0, 90).total_seconds())
# Negative timedelta normalisation: -1 microsecond -> -1 day, 23:59:59.999999
print(datetime.timedelta(0, 0, -1))
