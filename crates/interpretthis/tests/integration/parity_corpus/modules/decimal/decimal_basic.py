# Basic Decimal — exact decimal arithmetic.
#
# Pins CPython semantics: Decimal preserves the exact decimal
# representation of its string input (no binary-float roundoff).
# Decimal-int and Decimal-Decimal arithmetic stays exact.
from decimal import Decimal

a = Decimal("0.1")
b = Decimal("0.2")

# The classic float-roundoff trap: 0.1 + 0.2 != 0.3 in IEEE 754.
# Decimal arithmetic keeps it exact.
print(a + b)
print(a + b == Decimal("0.3"))

# Subtraction, multiplication, true division — all exact.
print(Decimal("1.5") - Decimal("0.7"))
print(Decimal("2.5") * Decimal("4"))
print(Decimal("10") / Decimal("4"))

# Int values coerce cleanly into Decimal arithmetic.
print(Decimal("3.14") * 2)
print(Decimal("100") + 50)

# Construction from string preserves the exact digit string in repr.
print(Decimal("3.14159"))
print(Decimal("0.000001"))
print(Decimal("-42"))
