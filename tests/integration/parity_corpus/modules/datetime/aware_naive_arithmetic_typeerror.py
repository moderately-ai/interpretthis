# Mixing aware and naive datetimes in arithmetic raises TypeError per
# CPython. Pins datetime::try_arith's aware/naive subtraction check.
import datetime
naive = datetime.datetime(2026, 1, 15, 14, 0, 0)
utc = datetime.timezone(datetime.timedelta(0, 0))
aware = datetime.datetime(2026, 1, 15, 14, 0, 0, 0, utc)
try:
    diff = aware - naive
    print("no error")
except TypeError as e:
    print("TypeError")
# Same in the other order
try:
    diff = naive - aware
    print("no error")
except TypeError as e:
    print("TypeError")
# Aware - aware works.
aware2 = datetime.datetime(2026, 1, 16, 14, 0, 0, 0, utc)
diff = aware2 - aware
print(diff.days)
