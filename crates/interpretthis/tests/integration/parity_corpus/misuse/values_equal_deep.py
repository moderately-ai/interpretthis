from decimal import Decimal
from fractions import Fraction
from datetime import date, datetime, timedelta, time
print(date(2024, 1, 1) == date(2024, 1, 1))
print([date(2024, 1, 1)] == [date(2024, 1, 1)])
print(date(2024, 1, 1) in [date(2024, 1, 2), date(2024, 1, 1)])
print(datetime(2024, 1, 1, 12) == datetime(2024, 1, 1, 12))
print(timedelta(hours=1) == timedelta(minutes=60))
print(time(10, 30) == time(10, 30))
print(Decimal("1.5") == Decimal("1.5"))
print([Decimal("1.5")] == [Decimal("1.5")])
print(Fraction(1, 2) == Fraction(1, 2))
print([Fraction(1, 2)] == [Fraction(1, 2)])
print(2**70 == 2**70)
print([2**70] == [2**70])
print(2**70 in [2**70])
print({date(2024, 1, 1): "a"}[date(2024, 1, 1)])
print(len({timedelta(hours=1), timedelta(minutes=60)}))
