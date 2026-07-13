# Pins: localcontext saves/restores prec.
from decimal import Decimal, getcontext, localcontext

getcontext().prec = 28
with localcontext() as ctx:
    ctx.prec = 5
    print(getcontext().prec)
print(getcontext().prec)
