# format() / __format__ dispatch across types and format specs.
from decimal import Decimal
from fractions import Fraction

print(format(42, "05d"), format(42, "b"), format(42, "x"), format(42, "o"))
print(format(3.14159, ".2f"), format(3.14159, "e"), format(1000000, ","), format(0.5, "%"))
print(format("hi", ">10"), format("hi", "^10"), format("hi", "*<8"))
print(format(Decimal("3.14159"), ".2f"), format(Decimal("1000"), ","))
# Decimal rounds EXACTLY (half-even), unlike the binary float 2.675 -> 2.67.
print(format(Decimal("2.675"), ".2f"), format(Decimal("3.1"), ".3f"))
print(format(Decimal("0.1234"), ".2%"), format(Decimal("1234.5"), ",.2f"))
print(format(Decimal("-3.14159"), ".2f"), format(Decimal("42"), "+.1f"))
print("{:,.2f}".format(Decimal("1234567.891")), format(Decimal("100"), ".2e"))
print(format(3, "+d"), format(-3, "+d"), format(3.5, "+.1f"))
print(f"{255:#x}", f"{255:#b}", f"{3.14159:.3g}", f"{-5:+d}")
print(f"{42:>{10}}", f"{3.14159:.{2}f}")
print("{:.2%}".format(0.1234), "{:,.2f}".format(1234567.891))
print(format(True, "d"), format(False, "d"))
