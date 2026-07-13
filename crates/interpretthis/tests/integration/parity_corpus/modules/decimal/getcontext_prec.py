# Pins: decimal.getcontext().prec read/write.
from decimal import Decimal, getcontext

print(getcontext().prec)
getcontext().prec = 6
print(getcontext().prec)
# Division uses active precision (CPython may differ slightly in rounding form).
x = Decimal(1) / Decimal(7)
print(str(x)[:8])
