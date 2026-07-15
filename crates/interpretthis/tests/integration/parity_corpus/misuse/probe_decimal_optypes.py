from decimal import Decimal
try:
    Decimal("1") + "x"
except TypeError as e:
    print("add", type(e).__name__)
try:
    Decimal("1") + 1.5
except TypeError:
    print("float add rejected")
print(Decimal("1") + 2)
print(Decimal("10") / Decimal("4"))
print(Decimal("1") < Decimal("2"))
try:
    Decimal("1") < "x"
except TypeError:
    print("lt rejected")
